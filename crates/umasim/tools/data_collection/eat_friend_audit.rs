//! 当前两套配卡：吃面后训练落点与友人使用审计。
use std::{env,sync::Mutex};
use anyhow::Result;
use rand::prelude::StdRng;
use serde::Serialize;
use umasim::{bench::{self,CardPickOpts,DeckComposition},game::{Game,InheritInfo,Person,PersonType,Trainer,ramen::{Operation,RamenAction,RamenGame,RamenStage}},gamedata::{EventChoice,EventData,init_global_with_config,ramen::RAMENDATA},trainer::RecommendedRamenTrainer,utils::{get_workspace_root,load_game_config}};
const UMA:u32=102601;const FRIEND:u32=303054;
const INHERIT:InheritInfo=InheritInfo{blue_count:[15,0,0,0,3],extra_count:[10,10,20,20,20,40]};
#[derive(Default)]struct Counts{eat:i32,post_train:i32,aligned:i32,misaligned:i32,post_nontrain:i32,friend_train:i32,friend_outing:i32,friend_train_after60:i32}
struct Audit{inner:RecommendedRamenTrainer,c:Mutex<Counts>}
impl Trainer<RamenGame> for Audit{
 fn select_action(&self,g:&RamenGame,a:&[RamenAction],r:&mut StdRng)->Result<usize>{let i=self.inner.select_action(g,a,r)?;let x=a[i];let mut c=self.c.lock().unwrap();
  if g.stage==RamenStage::RamenSelect&&x.ramen.is_some(){c.eat+=1;}
  if g.stage==RamenStage::Train{
   if x.operation==Operation::FriendOuting{c.friend_outing+=1;}
   if let Operation::Train(t)=x.operation{let tr=t as usize;let has_friend=g.distribution()[tr].iter().any(|&p|p>=0&&g.persons()[p as usize].person_type()==PersonType::ScenarioCard);if has_friend{c.friend_train+=1;if g.persons().iter().any(|p|p.person_type()==PersonType::ScenarioCard&&p.friendship()>=60){c.friend_train_after60+=1;}}
    if let Some(rid)=g.ramen.current_ramen{c.post_train+=1;let aligned=RAMENDATA.get().and_then(|d|d.ramen_region_effect.get(rid)).is_some_and(|e|e.at_trains.contains(&(tr as i32)));if aligned{c.aligned+=1}else{c.misaligned+=1}}
   }else if g.ramen.current_ramen.is_some(){c.post_nontrain+=1;}
  }
  Ok(i)}
 fn select_choice(&self,g:&RamenGame,x:&[Vec<EventChoice>],r:&mut StdRng)->Result<usize>{self.inner.select_choice(g,x,r)}
 fn select_event_choice(&self,g:&RamenGame,e:&EventData,x:&[Vec<EventChoice>],r:&mut StdRng)->Result<usize>{self.inner.select_event_choice(g,e,x,r)}
}
fn trainer(build:&str)->RecommendedRamenTrainer{if build=="3速1耐1智"{RecommendedRamenTrainer::with_random_decimal_overrides([30.5270,24.1121,3.1265],1.1835,1.3989,231.2443,.1104,.2021,.4123,6.0798,12.6522,0.)}else{RecommendedRamenTrainer::with_random_decimal_overrides([27.8462,26.0708,2.4427],1.0824,1.3875,226.5387,.1039,.2071,8.4305,4.2105,10.6249,0.)}}
#[derive(Serialize)]struct Row{build:String,run:u64,eat_archived:i32,eat_seen:i32,post_train:i32,aligned:i32,misaligned:i32,post_nontrain:i32,friend_train:i32,friend_train_after60:i32,friend_outing:i32,wisdom_cards:i32,score:i32}
fn main()->Result<()>{env::set_current_dir(get_workspace_root()?)?;init_global_with_config(&load_game_config()?)?;let reps=bench::select_representatives(&CardPickOpts::default())?;let builds=[("3速1耐1智",[3,1,0,0,1]),("2速2耐1智",[2,2,0,0,1])];let mut w=csv::Writer::from_path("eat-friend-audit.csv")?;for(build,counts)in builds{let deck=DeckComposition{counts,name:String::new()}.build_deck(&reps.picked,FRIEND)?;for run in 0..100{let audit=Audit{inner:trainer(build),c:Mutex::new(Counts::default())};let(mut rng,master)=bench::seeded_rngs(606060,run);let mut g=RamenGame::newgame(UMA,&deck,INHERIT)?;g.set_rule_master(master);g.run_full_game(&audit,&mut rng)?;let c=audit.c.into_inner().unwrap();w.serialize(Row{build:build.into(),run,eat_archived:g.ramen.yearly_eat_count.iter().sum(),eat_seen:c.eat,post_train:c.post_train,aligned:c.aligned,misaligned:c.misaligned,post_nontrain:c.post_nontrain,friend_train:c.friend_train,friend_train_after60:c.friend_train_after60,friend_outing:c.friend_outing,wisdom_cards:counts[4]as i32,score:g.uma.calc_score()})?;}}w.flush()?;Ok(())}
