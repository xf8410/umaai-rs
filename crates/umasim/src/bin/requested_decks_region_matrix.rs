//! PR #15 最终 H0/G39 小数策略：两配卡 × 第三年120地区 × 每组合100同seed局。
use std::{env, path::Path};
use anyhow::{Context, Result, ensure};
use rand::prelude::StdRng;
use serde::Serialize;
use umasim::{
    bench::{self, CardPickOpts, DeckComposition},
    game::{Game, InheritInfo, Trainer, ramen::{Operation, RamenAction, RamenGame, rules::get_region_combinations}},
    gamedata::{EventChoice, EventData, init_global_with_config},
    trainer::RecommendedRamenTrainer,
    utils::{get_workspace_root, load_game_config},
};
const UMA:u32=102601; const FRIEND:u32=303054; const BASE_SEED:u64=884_400;
const INHERIT:InheritInfo=InheritInfo{blue_count:[15,0,0,0,3],extra_count:[10,10,20,20,20,40]};
#[derive(Serialize)]
struct Row{build:String,composition:String,deck:String,combo_index:usize,region_ids:String,run:usize,score:i32,skill_pt:i32,speed:i32,stamina:i32,power:i32,guts:i32,wisdom:i32}
fn h0()->RecommendedRamenTrainer{
    RecommendedRamenTrainer::with_random_decimal_overrides(
        [31.0594,26.2536,4.3471],1.1311,1.3254,234.9200,
        0.1148,0.1999,5.2663,5.2657,11.2926,0.0)
}
struct FixedY3{inner:RecommendedRamenTrainer,combo:[usize;3]}
impl Trainer<RamenGame> for FixedY3{
 fn select_action(&self,g:&RamenGame,a:&[RamenAction],r:&mut StdRng)->Result<usize>{
  if g.turn()==47&&a.iter().all(|x|matches!(x.operation,Operation::RegionSelect(_))){return a.iter().position(|x|x.operation==Operation::RegionSelect(self.combo)).with_context(||format!("第三年组合缺失: {:?}",self.combo));}
  self.inner.select_action(g,a,r)
 }
 fn select_choice(&self,g:&RamenGame,c:&[Vec<EventChoice>],r:&mut StdRng)->Result<usize>{self.inner.select_choice(g,c,r)}
 fn select_event_choice(&self,g:&RamenGame,e:&EventData,c:&[Vec<EventChoice>],r:&mut StdRng)->Result<usize>{self.inner.select_event_choice(g,e,c,r)}
 fn last_breakdown(&self)->Option<String>{self.inner.last_breakdown()}
}
fn main()->Result<()>{
 env::set_current_dir(get_workspace_root()?)?;init_global_with_config(&load_game_config()?)?;
 let start:usize=env::var("COMBO_START")?.parse()?;let end:usize=env::var("COMBO_END")?.parse()?;let runs:usize=env::var("RUNS").unwrap_or_else(|_|"100".into()).parse()?;
 ensure!(runs==100,"正式矩阵必须每格100局");let combos=get_region_combinations(2)?;ensure!(combos.len()==120,"应有120组合");ensure!(start<end&&end<=120,"分片无效");
 let reps=bench::select_representatives(&CardPickOpts::default())?;
 let builds=[("3速1耐1智",[3,1,0,0,1]),("2速2耐1智",[2,2,0,0,1])];
 let mut w=csv::Writer::from_path(Path::new("requested-decks-region-matrix.csv"))?;
 for(build,counts)in builds{let comp=DeckComposition{counts,name:String::new()};let deck=comp.build_deck(&reps.picked,FRIEND)?;let deck_text=deck.iter().map(u32::to_string).collect::<Vec<_>>().join("/");
  for(offset,combo)in combos[start..end].iter().enumerate(){let ci=start+offset;let ids=combo.iter().map(usize::to_string).collect::<Vec<_>>().join("/");
   for run in 0..runs{let(mut rng,master)=bench::seeded_rngs(BASE_SEED,run as u64);let mut g=RamenGame::newgame(UMA,&deck,INHERIT)?;g.set_rule_master(master);g.run_full_game(&FixedY3{inner:h0(),combo:*combo},&mut rng)?;let s=g.uma.five_status;
    w.serialize(Row{build:build.into(),composition:comp.name(),deck:deck_text.clone(),combo_index:ci,region_ids:ids.clone(),run,score:g.uma.calc_score(),skill_pt:g.uma.skill_pt,speed:s[0],stamina:s[1],power:s[2],guts:s[3],wisdom:s[4]})?;
   }
  }
 }
 w.flush()?;Ok(())
}
