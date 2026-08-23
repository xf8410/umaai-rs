//! 多维小数随机权重矩阵；候选参数由 workflow 确定性生成并显式传入。
use std::{env,path::Path};
use anyhow::Result;
use umasim::{bench::{self,CardPickOpts,DeckComposition},game::{Game,InheritInfo,ramen::RamenGame},gamedata::init_global_with_config,trainer::RecommendedRamenTrainer,utils::{get_workspace_root,load_game_config}};
const UMA:u32=102601;const FRIEND:u32=303054;
const INHERIT:InheritInfo=InheritInfo{blue_count:[15,0,0,0,3],extra_count:[10,10,20,20,20,40]};
fn f(k:&str)->Result<f32>{Ok(env::var(k)?.parse()?)}
fn trainer()->Result<RecommendedRamenTrainer>{Ok(RecommendedRamenTrainer::with_random_decimal_overrides([f("Y1_PT")?,f("Y2_PT")?,f("Y3_PT")?],f("GAP")?,f("OVER")?,f("SAC")?,f("Y12_WINDOW")?,f("Y3_WINDOW")?,f("RESERVE")?,f("BOND")?,f("HINT")?,f("CONTINUITY")?))}
fn deck()->Result<[u32;6]>{let c=DeckComposition{counts:[3,1,0,0,1],name:String::new()};let r=bench::select_representatives(&CardPickOpts::default())?;c.build_deck(&r.picked,FRIEND)}
fn main()->Result<()>{std::env::set_current_dir(get_workspace_root()?)?;init_global_with_config(&load_game_config()?)?;let id=env::var("候选编号")?;let seed:u64=env::var("基础种子")?.parse()?;let shard:u64=env::var("分片序号")?.parse()?;let runs:u64=env::var("每分片局数")?.parse()?;let deck=deck()?;let params=["Y1_PT","Y2_PT","Y3_PT","GAP","OVER","SAC","Y12_WINDOW","Y3_WINDOW","RESERVE","BOND","HINT","CONTINUITY"].map(|k|env::var(k).unwrap());let mut rows=Vec::with_capacity(runs as usize);for off in 0..runs{let index=shard*runs+off;let(mut rng,master)=bench::seeded_rngs(seed,index);let mut g=RamenGame::newgame(UMA,&deck,INHERIT.clone())?;g.set_rule_master(master);g.run_full_game(&trainer()?,&mut rng)?;let mut row=vec![id.clone(),seed.to_string(),index.to_string(),g.uma.calc_score().to_string(),g.uma.skill_pt.to_string(),g.ramen.scenario_pt.to_string()];row.extend(params.clone());rows.push(row);}bench::write_csv(Path::new("随机小数权重矩阵.csv"),&["候选","基础种子","局序号","总分","技能点","拉面点","Y1PT","Y2PT","Y3PT","缺口","溢出","让分","前两年对盘","第三年对盘","预留","羁绊","Hint","续航"],&rows)}
