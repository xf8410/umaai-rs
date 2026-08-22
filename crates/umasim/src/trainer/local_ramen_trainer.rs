//! B 策略：上游 RamenPolicy 基准 + 可解释的本地长期收益修正。
use std::sync::Mutex;
use anyhow::Result;
use rand::prelude::StdRng;
use crate::{
    game::{FriendOutState, Game, Person, PersonType, Trainer,
        ramen::{Operation, RamenAction, RamenGame, RamenStage, policy::{RamenPolicy, RamenPolicyOutput}}},
    gamedata::{EventChoice, EventData}
};

#[derive(Debug, Clone)]
pub struct LocalRamenConfig {
    pub early_bond_value: f32, pub hint_bonus: f32,
    pub first_friend_click_value: f32, pub low_friend_bond_value: f32,
    pub active_friend_value: f32, pub high_fail_penalty: f32,
    pub feeling_overflow_threshold: i32, pub overflow_value: f32,
    pub rmj_urgency_margin: i32, pub rmj_urgency_bonus: f32,
    /// 本地长期价值最多允许放弃多少上游即时训练分。
    pub max_base_score_sacrifice: f32
}
impl Default for LocalRamenConfig {
    fn default() -> Self { Self {
        early_bond_value:8.0,hint_bonus:6.0,first_friend_click_value:90.0,
        low_friend_bond_value:35.0,active_friend_value:8.0,high_fail_penalty:700.0,
        feeling_overflow_threshold:10,overflow_value:8.0,rmj_urgency_margin:450,
        rmj_urgency_bonus:60.0,max_base_score_sacrifice:120.0
    }}
}
pub struct LocalRamenTrainer { pub policy:RamenPolicy,pub config:LocalRamenConfig,last_breakdown:Mutex<Option<String>> }
impl Default for LocalRamenTrainer { fn default()->Self{Self{policy:RamenPolicy::default(),config:LocalRamenConfig::default(),last_breakdown:Mutex::new(None)}}}
impl LocalRamenTrainer {
    pub fn new()->Self{Self::default()}
    fn choose(o:&[RamenPolicyOutput])->usize{o.iter().enumerate().max_by(|(ia,a),(ib,b)|a.score.total_cmp(&b.score).then_with(||ib.cmp(ia))).map(|(i,_)|i).unwrap_or(0)}
    fn stash(&self,o:&[RamenPolicyOutput]){let s=o.iter().enumerate().map(|(i,x)|format!("#{i} {:.0}[{}]",x.score,x.reason)).collect::<Vec<_>>().join(" | ");if let Ok(mut x)=self.last_breakdown.lock(){*x=Some(s)}}
    fn phase(t:i32)->f32{if t<24{1.0}else if t<48{0.55}else{0.15}}
    fn decide_train(&self,g:&RamenGame,a:&[RamenAction])->Result<(usize,Vec<RamenPolicyOutput>)>{
        let(guard,mut o)=self.policy.decide_train(g,a)?;if o.len()!=a.len(){return Ok((guard,o))}
        let base=o.iter().map(|x|x.score).collect::<Vec<_>>();let base_best=Self::choose(&o);let scale=Self::phase(g.turn());
        for(action,out)in a.iter().zip(o.iter_mut()){
            let Operation::Train(tt)=action.operation else{continue};let train=tt as usize;
            let people=g.distribution().get(train).into_iter().flatten().copied().filter(|&p|p>=0&&(p as usize)<g.persons().len()).map(|p|p as usize);
            let mut long=0.0;for i in people{let p=&g.persons()[i];match p.person_type(){
                PersonType::ScenarioCard=>long+=match g.friend.out_state{FriendOutState::UnClicked=>self.config.first_friend_click_value,_ if p.friendship()<60=>self.config.low_friend_bond_value*scale,_=>self.config.active_friend_value},
                PersonType::Card if p.friendship()<80=>{let mut gain:f32=if g.uma.flags.aijiao{9.0}else{7.0};if p.hint(){gain+=5.0}gain=gain.min((80-p.friendship())as f32);long+=gain*self.config.early_bond_value*scale;if p.hint(){long+=self.config.hint_bonus}},
                PersonType::Card if p.hint()=>long+=self.config.hint_bonus,_=>{}}}
            out.score+=long;out.add("local_long_term",long);let buffs=g.calc_training_buff(train)?;let fail=g.calc_training_failure_rate(&buffs,train);
            if fail>15.0{let pen=-((fail-15.0)/85.0).clamp(0.0,1.0)*self.config.high_fail_penalty;out.score+=pen;out.add("local_high_fail_tail",pen)}
        }
        let local=Self::choose(&o);let sacrifice=base[base_best]-base[local];let chosen=if sacrifice<=self.config.max_base_score_sacrifice{local}else{base_best};
        if sacrifice>self.config.max_base_score_sacrifice{o[chosen].reason.push_str(&format!(";保护:牺牲基础分{sacrifice:.0}>上限{:.0}",self.config.max_base_score_sacrifice))}Ok((chosen,o))
    }
    fn decide_ramen(&self,g:&RamenGame,a:&[RamenAction])->Result<(usize,Vec<RamenPolicyOutput>)>{
        let(_,mut o)=self.policy.decide_ramen(g,a)?;let stock:i32=g.ramen.feeling_stock.iter().sum();let overflow=(stock-self.config.feeling_overflow_threshold).max(0)as f32;let y=(g.current_year()-1).clamp(0,2)as usize;let gap=[1500,3000,3500][y]-g.ramen.scenario_pt;
        for(action,out)in a.iter().zip(o.iter_mut()){if action.ramen.is_none(){continue}let bonus=overflow*self.config.overflow_value;out.score+=bonus;out.add("local_stock_overflow",bonus);if gap>0&&gap<=self.config.rmj_urgency_margin{let close=1.0-gap as f32/self.config.rmj_urgency_margin as f32;let u=self.config.rmj_urgency_bonus*(0.5+0.5*close);out.score+=u;out.add("local_rmj_urgency",u)}}Ok((Self::choose(&o),o))
    }
}
impl Trainer<RamenGame> for LocalRamenTrainer{
    fn select_action(&self,g:&RamenGame,a:&[RamenAction],_:&mut StdRng)->Result<usize>{if a.len()<=1{return Ok(0)}let(i,o)=match g.stage{RamenStage::Train=>self.decide_train(g,a)?,RamenStage::RamenSelect=>self.decide_ramen(g,a)?,RamenStage::SpecialSelect=>self.policy.decide_special(g,a)?,RamenStage::RegionSelect=>{let y=match g.turn(){2=>0,23=>1,47=>2,_=>0};self.policy.decide_region(g,y,a)?},_=>(0,Vec::new())};self.stash(&o);Ok(i)}
    fn select_choice(&self,g:&RamenGame,c:&[Vec<EventChoice>],_:&mut StdRng)->Result<usize>{let(i,o)=self.policy.decide_event(g,c)?;self.stash(&o);Ok(i)}
    fn select_event_choice(&self,g:&RamenGame,_:&EventData,c:&[Vec<EventChoice>],r:&mut StdRng)->Result<usize>{self.select_choice(g,c,r)}
    fn last_breakdown(&self)->Option<String>{self.last_breakdown.lock().ok().and_then(|x|x.clone())}
}
