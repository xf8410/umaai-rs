//! 当前正式手写策略 vs 扁平蒙特卡洛：Train 阶段非侵入式 regret / 体力审计。
use std::{env, path::Path, sync::Mutex};
use anyhow::{Context, Result};
use rand::prelude::StdRng;
use serde::Serialize;
use umasim::{
    bench::{self, CardPickOpts, DeckComposition},
    game::{Game, InheritInfo, Trainer, ramen::{Operation, RamenAction, RamenGame, RamenStage}},
    gamedata::{EventChoice, EventData, init_global_with_config},
    search::{FlatSearch, SearchConfig},
    trainer::RecommendedRamenTrainer,
    utils::{get_workspace_root, load_game_config},
};
const UMA:u32=102601; const FRIEND:u32=303054;
const INHERIT:InheritInfo=InheritInfo{blue_count:[15,0,0,0,3],extra_count:[10,10,20,20,20,40]};
#[derive(Serialize,Clone)]
struct DecisionRow { build:String, run:u64, turn:i32, year:i32, vital:i32, max_vital:i32,
    hand_action:String, teacher_action:String, agree:bool, hand_mean:f64, teacher_mean:f64, regret:f64,
    hand_post_vital:i32, teacher_post_vital:i32, hand_recovery:bool, teacher_recovery:bool }
#[derive(Serialize)]
struct GameRow { build:String,run:u64,score:i32,skill_pt:i32,speed:i32,stamina:i32,power:i32,guts:i32,wisdom:i32,searched:usize,disagree:usize,total_regret:f64,recovery_disagree:usize }
struct AuditTrainer { inner:RecommendedRamenTrainer, search:FlatSearch<RamenGame>, build:String, run:u64, rows:Mutex<Vec<DecisionRow>> }
impl AuditTrainer {
 fn post_vital(g:&RamenGame,a:&RamenAction)->Result<i32>{Ok(match a.operation{Operation::Train(tt)=>{let tr=tt as usize;let b=g.calc_training_buff(tr)?;g.uma.vital+g.calc_training_value(&b,tr)?.vital},Operation::Rest=>g.uma.max_vital.min(g.uma.vital+50),_=>g.uma.vital})}
 fn recovery(a:&RamenAction)->bool{matches!(a.operation,Operation::Rest|Operation::FriendOuting)}
}
impl Trainer<RamenGame> for AuditTrainer {
 fn select_action(&self,g:&RamenGame,a:&[RamenAction],rng:&mut StdRng)->Result<usize>{
  let hand=self.inner.select_action(g,a,rng)?;
  if g.stage==RamenStage::Train && a.len()>1 {
   let mut shadow=rng.clone();
   let out=self.search.search(g,a,&mut shadow)?;
   let teacher=out.best_action_idx;
   let hm=out.action_results.get(hand).map(|x|x.0.mean()).unwrap_or(f64::NAN);
   let tm=out.action_results.get(teacher).map(|x|x.0.mean()).unwrap_or(f64::NAN);
   let row=DecisionRow{build:self.build.clone(),run:self.run,turn:g.turn(),year:g.current_year(),vital:g.uma.vital,max_vital:g.uma.max_vital,
    hand_action:a[hand].operation.to_string(),teacher_action:a[teacher].operation.to_string(),agree:hand==teacher,hand_mean:hm,teacher_mean:tm,regret:tm-hm,
    hand_post_vital:Self::post_vital(g,&a[hand])?,teacher_post_vital:Self::post_vital(g,&a[teacher])?,hand_recovery:Self::recovery(&a[hand]),teacher_recovery:Self::recovery(&a[teacher])};
   self.rows.lock().map_err(|_|anyhow::anyhow!("审计锁损坏"))?.push(row);
  }
  Ok(hand)
 }
 fn select_choice(&self,g:&RamenGame,c:&[Vec<EventChoice>],r:&mut StdRng)->Result<usize>{self.inner.select_choice(g,c,r)}
 fn select_event_choice(&self,g:&RamenGame,e:&EventData,c:&[Vec<EventChoice>],r:&mut StdRng)->Result<usize>{self.inner.select_event_choice(g,e,c,r)}
}
fn deck(name:&str)->Result<[u32;6]>{let counts=match name{"3速1耐1智"=>[3,1,0,0,1],"2速2耐1智"=>[2,2,0,0,1],_=>anyhow::bail!("未知配卡 {name}")};let c=DeckComposition{counts,name:name.into()};let r=bench::select_representatives(&CardPickOpts::default())?;c.build_deck(&r.picked,FRIEND)}
fn main()->Result<()> {
 env::set_current_dir(get_workspace_root()?)?;init_global_with_config(&load_game_config()?)?;
 let build=env::var("BUILD").context("缺少 BUILD")?;let shard:u64=env::var("SHARD").unwrap_or_else(|_|"0".into()).parse()?;let runs:u64=env::var("RUNS").unwrap_or_else(|_|"2".into()).parse()?;let n:usize=env::var("SEARCH_N").unwrap_or_else(|_|"32".into()).parse()?;let base:u64=env::var("BASE_SEED").unwrap_or_else(|_|"717171".into()).parse()?;
 let deck=deck(&build)?;let mut decisions=Vec::new();let mut games=Vec::new();
 for off in 0..runs {let run=shard*runs+off;let(mut rng,master)=bench::seeded_rngs(base,run);let mut g=RamenGame::newgame(UMA,&deck,INHERIT)?;g.set_rule_master(master);
  let t=AuditTrainer{inner:RecommendedRamenTrainer::new(),search:FlatSearch::new(SearchConfig::default().with_search_n(n).with_ucb(false)),build:build.clone(),run,rows:Mutex::new(Vec::new())};
  g.run_full_game(&t,&mut rng)?;let rows=t.rows.into_inner().map_err(|_|anyhow::anyhow!("审计锁损坏"))?;let disagree=rows.iter().filter(|x|!x.agree).count();let recovery_disagree=rows.iter().filter(|x|x.hand_recovery!=x.teacher_recovery).count();let total_regret=rows.iter().map(|x|x.regret).sum();let s=g.uma.five_status;
  games.push(GameRow{build:build.clone(),run,score:g.uma.calc_score(),skill_pt:g.uma.skill_pt,speed:s[0],stamina:s[1],power:s[2],guts:s[3],wisdom:s[4],searched:rows.len(),disagree,total_regret,recovery_disagree});decisions.extend(rows);
 }
 let mut w=csv::Writer::from_path(Path::new("train-regret-decisions.csv"))?;for r in decisions{w.serialize(r)?}w.flush()?;let mut w=csv::Writer::from_path(Path::new("train-regret-games.csv"))?;for r in games{w.serialize(r)?}w.flush()?;Ok(())
}
