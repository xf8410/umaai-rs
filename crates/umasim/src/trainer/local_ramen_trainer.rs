//! 拉面杯策略：在现有 `RamenPolicy` 即时评分上增加受保护的长期收益修正。
//!
//! 结构实验同时移植旧 UmaAi 手写策略中可跨场景复用的方法：未来属性预留、动态体力价值、
//! Hint 随机命中概率和分级失败期望。具体机甲场景常数与动作不复制。

use std::sync::Mutex;

use anyhow::Result;
use rand::prelude::StdRng;

use crate::{
    game::{
        FriendOutState, Game, Person, PersonType, Trainer,
        ramen::{
            Operation, RamenAction, RamenGame, RamenStage,
            policy::{RamenPolicy, RamenPolicyConfig, RamenPolicyOutput}
        }
    },
    gamedata::{EventChoice, EventData}
};

#[derive(Debug, Clone)]
pub struct LocalRamenConfig {
    pub early_bond_value: f32,
    pub hint_bonus: f32,
    pub first_friend_click_value: f32,
    pub low_friend_bond_value: f32,
    pub active_friend_value: f32,
    pub high_fail_penalty: f32,
    pub feeling_overflow_threshold: i32,
    pub overflow_value: f32,
    pub max_base_score_sacrifice: f32,
    /// 为未来固定奖励和后续训练预留的属性空间；0 表示关闭。
    pub status_reserve_max: f32,
    /// 使用随剩余回合变化的体力边际价值。
    pub dynamic_vital: bool,
    /// 同一训练多个 Hint 时按随机命中概率折算，而非逐个全额相加。
    pub probabilistic_hint: bool,
    /// 使用小失败/大失败两层期望损失。
    pub expected_fail: bool
}

impl Default for LocalRamenConfig {
    fn default() -> Self {
        Self {
            early_bond_value: 8.0,
            hint_bonus: 6.0,
            first_friend_click_value: 75.0,
            low_friend_bond_value: 35.0,
            active_friend_value: 8.0,
            high_fail_penalty: 0.0,
            feeling_overflow_threshold: 8,
            overflow_value: 8.0,
            max_base_score_sacrifice: 140.0,
            status_reserve_max: 0.0,
            dynamic_vital: false,
            probabilistic_hint: false,
            expected_fail: false
        }
    }
}

pub struct LocalRamenTrainer {
    policy: RamenPolicy,
    config: LocalRamenConfig,
    last_breakdown: Mutex<Option<String>>
}

impl Default for LocalRamenTrainer {
    fn default() -> Self {
        Self::with_configs(RamenPolicyConfig::default(), LocalRamenConfig::default())
    }
}

impl LocalRamenTrainer {
    pub fn new() -> Self { Self::default() }

    pub fn with_configs(policy: RamenPolicyConfig, config: LocalRamenConfig) -> Self {
        Self { policy: RamenPolicy::new(policy), config, last_breakdown: Mutex::new(None) }
    }

    /// 名称至少包含 `ptN-sacN-long-failN`。可追加结构 token：
    /// `reserve20|reserve40`、`vital`、`hintprob`、`failmodel`、`structall`。
    pub fn matrix_variant(name: &str) -> Result<Self> {
        let mut policy = RamenPolicyConfig::default();
        let mut local = LocalRamenConfig::default();
        let (mut seen_pt, mut seen_sac, mut seen_mode, mut seen_fail) = (false, false, false, false);
        for token in name.split('-') {
            if let Some(value) = token.strip_prefix("pt") {
                policy.pt_rate = value.parse()?; seen_pt = true;
            } else if let Some(value) = token.strip_prefix("sac") {
                local.max_base_score_sacrifice = value.parse()?; seen_sac = true;
            } else if let Some(value) = token.strip_prefix("fail") {
                local.high_fail_penalty = value.parse()?; seen_fail = true;
            } else if let Some(value) = token.strip_prefix("reserve") {
                local.status_reserve_max = value.parse()?;
            } else if token == "vital" {
                local.dynamic_vital = true;
            } else if token == "hintprob" {
                local.probabilistic_hint = true;
            } else if token == "failmodel" {
                local.expected_fail = true;
            } else if token == "structall" {
                local.status_reserve_max = 40.0;
                local.dynamic_vital = true;
                local.probabilistic_hint = true;
                local.expected_fail = true;
            } else if token == "plain" {
                local.early_bond_value = 0.0; local.hint_bonus = 0.0;
                local.first_friend_click_value = 0.0; local.low_friend_bond_value = 0.0;
                local.active_friend_value = 0.0; local.overflow_value = 0.0; seen_mode = true;
            } else if token == "long" || token == "base" {
                seen_mode = true;
            } else {
                anyhow::bail!("未知矩阵变体字段: {token} ({name})");
            }
        }
        if !(seen_pt && seen_sac && seen_mode && seen_fail) { anyhow::bail!("矩阵变体字段不完整: {name}"); }
        Ok(Self::with_configs(policy, local))
    }

    fn choose(outputs: &[RamenPolicyOutput]) -> usize {
        outputs.iter().enumerate().max_by(|(li,l),(ri,r)| l.score.total_cmp(&r.score).then_with(||ri.cmp(li))).map(|(i,_)|i).unwrap_or(0)
    }
    fn stash(&self, outputs: &[RamenPolicyOutput]) {
        let text=outputs.iter().enumerate().map(|(i,o)|format!("#{i} {:.0}[{}]",o.score,o.reason)).collect::<Vec<_>>().join(" | ");
        if let Ok(mut b)=self.last_breakdown.lock(){*b=Some(text);}
    }
    fn phase_scale(turn:i32)->f32 { if turn<24 {1.0} else if turn<48 {0.55} else {0.15} }

    /// 旧 UmaAi 的软控属性思想：越早预留越多未来空间；越接近上限，当前增益折价越大。
    fn reserve_penalty(&self, game:&RamenGame, gain:&[i32;6])->f32 {
        if self.config.status_reserve_max<=0.0 { return 0.0; }
        let remain=(76-game.turn()).max(0) as f32;
        let reserve=self.config.status_reserve_max*remain/76.0;
        let mut penalty=0.0;
        for i in 0..5 {
            let headroom=(game.uma.five_status_limit[i]-game.uma.five_status[i]).max(0) as f32;
            let before=(reserve-headroom).max(0.0);
            let after=(reserve-(headroom-gain[i] as f32)).max(0.0);
            penalty+=(after*after-before*before)/(2.0*reserve.max(1.0));
        }
        penalty*6.0
    }

    fn dynamic_vital_factor(turn:i32)->f32 {
        // 早期体力用于更多未来训练；终盘剩余体力价值快速清零。
        if turn>=72 {0.25} else {3.5+(turn as f32/72.0)*2.0}
    }

    fn decide_train(&self, game:&RamenGame, actions:&[RamenAction])->Result<(usize,Vec<RamenPolicyOutput>)>{
        let (guard_choice,mut outputs)=self.policy.decide_train(game,actions)?;
        if outputs.len()!=actions.len(){return Ok((guard_choice,outputs));}
        let base_scores=outputs.iter().map(|o|o.score).collect::<Vec<_>>();
        let base_best=Self::choose(&outputs);
        let phase=Self::phase_scale(game.turn());
        for (action,output) in actions.iter().zip(outputs.iter_mut()) {
            let Operation::Train(tt)=action.operation else {continue};
            let training=tt as usize;
            let buffs=game.calc_training_buff(training)?;
            let value=game.calc_training_value(&buffs,training)?;
            let people=game.distribution().get(training).into_iter().flatten().copied()
                .filter(|&p|p>=0&&(p as usize)<game.persons().len()).map(|p|p as usize).collect::<Vec<_>>();
            let hinted=people.iter().filter(|&&i| game.persons()[i].hint()&&matches!(game.persons()[i].person_type(),PersonType::Card)).count();
            let hint_prob=if self.config.probabilistic_hint&&hinted>0 {1.0/hinted as f32}else{1.0};
            let mut long_term=0.0;
            for person_index in people {
                let person=&game.persons()[person_index];
                match person.person_type(){
                    PersonType::ScenarioCard=>long_term+=match game.friend.out_state{
                        FriendOutState::UnClicked=>self.config.first_friend_click_value,
                        _ if person.friendship()<60=>self.config.low_friend_bond_value*phase,
                        _=>self.config.active_friend_value
                    },
                    PersonType::Card if person.friendship()<80=>{
                        let mut bond=if game.uma.flags.aijiao{9.0}else{7.0};
                        if person.hint(){bond+=5.0*hint_prob;}
                        bond=bond.min((80-person.friendship()) as f32);
                        long_term+=bond*self.config.early_bond_value*phase;
                        if person.hint(){long_term+=self.config.hint_bonus*hint_prob;}
                    }
                    PersonType::Card if person.hint()=>long_term+=self.config.hint_bonus*hint_prob,
                    _=>{}
                }
            }
            output.score+=long_term; output.add("local_long_term",long_term);

            let reserve=-self.reserve_penalty(game,&value.status_pt);
            output.score+=reserve; output.add("future_status_reserve",reserve);

            if self.config.dynamic_vital {
                let cost=(-value.vital).max(0) as f32;
                let adjust=-cost*(Self::dynamic_vital_factor(game.turn())-self.policy.config.train_vital_value);
                output.score+=adjust; output.add("dynamic_vital",adjust);
            }
            let failure=game.calc_training_failure_rate(&buffs,training);
            if self.config.expected_fail&&failure>0.0 {
                let p=failure/100.0;
                let big_p=if failure>=20.0 {p}else{0.0};
                let extra=-p*(150.0+big_p*(500.0-150.0)-self.policy.config.failure_penalty);
                output.score+=extra; output.add("expected_fail_layers",extra);
            } else if failure>15.0&&self.config.high_fail_penalty>0.0 {
                let penalty=-((failure-15.0)/85.0).clamp(0.0,1.0)*self.config.high_fail_penalty;
                output.score+=penalty; output.add("local_high_fail_tail",penalty);
            }
        }
        let local_best=Self::choose(&outputs);
        let sacrifice=base_scores[base_best]-base_scores[local_best];
        let choice=if sacrifice<=self.config.max_base_score_sacrifice{local_best}else{base_best};
        if sacrifice>self.config.max_base_score_sacrifice {outputs[choice].reason.push_str(&format!(";保护:结构修正牺牲基础分{sacrifice:.0}>上限{:.0}",self.config.max_base_score_sacrifice));}
        Ok((choice,outputs))
    }

    fn decide_ramen(&self,game:&RamenGame,actions:&[RamenAction])->Result<(usize,Vec<RamenPolicyOutput>)>{
        let (_,mut outputs)=self.policy.decide_ramen(game,actions)?;
        let risk=(game.ramen.feeling_stock.iter().sum::<i32>()-self.config.feeling_overflow_threshold).max(0) as f32;
        for (a,o) in actions.iter().zip(outputs.iter_mut()){if a.ramen.is_some(){let b=risk*self.config.overflow_value;o.score+=b;o.add("local_stock_pressure",b);}}
        Ok((Self::choose(&outputs),outputs))
    }
}

impl Trainer<RamenGame> for LocalRamenTrainer {
    fn select_action(&self,game:&RamenGame,actions:&[RamenAction],_rng:&mut StdRng)->Result<usize>{
        if actions.len()<=1{return Ok(0);} let (choice,outputs)=match game.stage{
            RamenStage::Train=>self.decide_train(game,actions)?, RamenStage::RamenSelect=>self.decide_ramen(game,actions)?,
            RamenStage::SpecialSelect=>self.policy.decide_special(game,actions)?, RamenStage::RegionSelect=>{let y=match game.turn(){2=>0,23=>1,47=>2,_=>0};self.policy.decide_region(game,y,actions)?}, _=>(0,Vec::new())};
        self.stash(&outputs);Ok(choice)
    }
    fn select_choice(&self,game:&RamenGame,choices:&[Vec<EventChoice>],_rng:&mut StdRng)->Result<usize>{let(c,o)=self.policy.decide_event(game,choices)?;self.stash(&o);Ok(c)}
    fn select_event_choice(&self,game:&RamenGame,_event:&EventData,choices:&[Vec<EventChoice>],rng:&mut StdRng)->Result<usize>{self.select_choice(game,choices,rng)}
    fn last_breakdown(&self)->Option<String>{self.last_breakdown.lock().ok().and_then(|b|b.clone())}
}
