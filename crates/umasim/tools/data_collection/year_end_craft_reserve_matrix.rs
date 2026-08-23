//! P0第5步：临近RMJ时保留至少一碗可制作地区面的矩阵。
use std::{env,path::Path};
use anyhow::Result;
use umasim::{bench::{self,CardPickOpts,DeckComposition},game::{Game,InheritInfo,ramen::RamenGame},gamedata::init_global_with_config,trainer::RecommendedRamenTrainer,utils::{get_workspace_root,load_game_config}};
const UMA:u32=102601;const FRIEND:u32=303054;
const INHERIT:InheritInfo=InheritInfo{blue_count:[15,0,0,0,3],extra_count:[10,10,20,20,20,40]};
fn candidate(name:&str)->Result<RecommendedRamenTrainer>{
 let (w,p)=match name{"保线关"=>(0,0.),"W2P80"=>(2,80.),"W2P160"=>(2,160.),"W3P80"=>(3,80.),"W3P160"=>(3,160.),"W4P80"=>(4,80.),"W4P160"=>(4,160.),"W5P160"=>(5,160.),"W3硬保线"=>(3,1_000_000.),"W5硬保线"=>(5,1_000_000.),_=>anyhow::bail!("未知候选:{name}")};
 Ok(RecommendedRamenTrainer::with_year_end_craft_reserve_override(w,p))
}
fn deck()->Result<[u32;6]>{let c=DeckComposition{counts:[3,1,0,0,1],name:String::new()};let r=bench::select_representatives(&CardPickOpts::default())?;c.build_deck(&r.picked,FRIEND)}
fn main()->Result<()>{std::env::set_current_dir(get_workspace_root()?)?;init_global_with_config(&load_game_config()?)?;let name=env::var("候选方案")?;let seed:u64=env::var("基础种子")?.parse()?;let shard:u64=env::var("分片序号")?.parse()?;let runs:u64=env::var("每分片局数")?.parse()?;let deck=deck()?;let mut rows=Vec::with_capacity(runs as usize);for off in 0..runs{let idx=shard*runs+off;let(mut rng,master)=bench::seeded_rngs(seed,idx);let mut game=RamenGame::newgame(UMA,&deck,INHERIT.clone())?;game.set_rule_master(master);game.run_full_game(&candidate(&name)?,&mut rng)?;rows.push(vec![name.clone(),seed.to_string(),idx.to_string(),game.uma.calc_score().to_string(),game.uma.skill_pt.to_string(),game.ramen.rmj_results.iter().filter(|&&x|x).count().to_string()]);}bench::write_csv(Path::new("年末保线矩阵.csv"),&["方案","基础种子","局序号","总分","技能点","RMJ成功年数"],&rows)}
