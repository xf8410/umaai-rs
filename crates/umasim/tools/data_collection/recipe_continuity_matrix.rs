//! P0 第4步：吃完当前面后的配方续航成本矩阵。

use std::{env,path::Path};
use anyhow::Result;
use umasim::{bench::{self,CardPickOpts,DeckComposition},game::{Game,InheritInfo,ramen::RamenGame},gamedata::init_global_with_config,trainer::RecommendedRamenTrainer,utils::{get_workspace_root,load_game_config}};
const UMA:u32=102601; const FRIEND:u32=303054;
const INHERIT:InheritInfo=InheritInfo{blue_count:[15,0,0,0,3],extra_count:[10,10,20,20,20,40]};
fn candidate(name:&str)->Result<RecommendedRamenTrainer>{
 let w=match name{"续航0"=>0.0,"续航5"=>5.0,"续航10"=>10.0,"续航15"=>15.0,"续航20"=>20.0,"续航30"=>30.0,"续航40"=>40.0,"续航60"=>60.0,_=>anyhow::bail!("未知候选: {name}")};
 Ok(RecommendedRamenTrainer::with_recipe_continuity_override(w))
}
fn deck()->Result<[u32;6]>{let c=DeckComposition{counts:[3,1,0,0,1],name:String::new()};let r=bench::select_representatives(&CardPickOpts::default())?;c.build_deck(&r.picked,FRIEND)}
fn main()->Result<()>{
 std::env::set_current_dir(get_workspace_root()?)?;init_global_with_config(&load_game_config()?)?;
 let name=env::var("候选方案")?;let seed:u64=env::var("基础种子")?.parse()?;let shard:u64=env::var("分片序号")?.parse()?;let runs:u64=env::var("每分片局数")?.parse()?;let deck=deck()?;let mut rows=Vec::with_capacity(runs as usize);
 for offset in 0..runs{let index=shard*runs+offset;let(mut rng,master)=bench::seeded_rngs(seed,index);let mut game=RamenGame::newgame(UMA,&deck,INHERIT.clone())?;game.set_rule_master(master);game.run_full_game(&candidate(&name)?,&mut rng)?;rows.push(vec![name.clone(),seed.to_string(),index.to_string(),game.uma.calc_score().to_string(),game.uma.skill_pt.to_string(),game.ramen.scenario_pt.to_string(),game.ramen.rmj_results.iter().filter(|&&x|x).count().to_string()]);}
 bench::write_csv(Path::new("配方续航矩阵.csv"),&["方案","基础种子","局序号","总分","技能点","最终拉面点","RMJ成功年数"],&rows)
}
