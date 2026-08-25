//! 3速1耐1智与2速2耐1智：100个相同seed配对比较，不强制地区选择。
use std::{env, path::Path};
use anyhow::{Context, Result, ensure};
use serde::Serialize;
use umasim::{bench::{self, CardPickOpts, DeckComposition}, game::InheritInfo, gamedata::init_global_with_config, trainer::{LoggingTrainer, RecommendedRamenTrainer}, utils::{get_workspace_root, load_game_config}};

const UMA:u32=102601;
const FRIEND:u32=303054;
const BASE_SEED:u64=884_400;
const INHERIT:InheritInfo=InheritInfo{blue_count:[15,0,0,0,3],extra_count:[10,10,20,20,20,40]};

#[derive(Serialize)]
struct Row{build:String,composition:String,deck:String,run:usize,score:i32,skill_pt:i32,speed:i32,stamina:i32,power:i32,guts:i32,wisdom:i32}

fn main()->Result<()>{
 env::set_current_dir(get_workspace_root()?)?;
 init_global_with_config(&load_game_config()?)?;
 let build=env::var("BUILD").context("缺少 BUILD")?;
 let runs:usize=env::var("RUNS").unwrap_or_else(|_|"100".into()).parse()?;
 ensure!(runs==100,"本验收要求严格100局，实际{runs}");
 let counts=match build.as_str(){
  "3速1耐1智"=>[3,1,0,0,1],
  "2速2耐1智"=>[2,2,0,0,1],
  _=>anyhow::bail!("未知BUILD: {build}"),
 };
 let comp=DeckComposition{counts,name:String::new()};
 let reps=bench::select_representatives(&CardPickOpts::default())?;
 let deck=comp.build_deck(&reps.picked,FRIEND)?;
 let deck_text=deck.iter().map(u32::to_string).collect::<Vec<_>>().join("/");
 let mut w=csv::Writer::from_path(Path::new("composition-pair-100.csv"))?;
 for run in 0..runs{
  let trainer=LoggingTrainer::new(RecommendedRamenTrainer::new(),run as u64);
  let out=bench::run_seeded(UMA,&deck,&INHERIT,BASE_SEED,run as u64,&trainer)?;
  let s=out.five_status;
  w.serialize(Row{build:build.clone(),composition:comp.name(),deck:deck_text.clone(),run,score:out.score,skill_pt:out.skill_pt,speed:s[0],stamina:s[1],power:s[2],guts:s[3],wisdom:s[4]})?;
 }
 w.flush()?;
 Ok(())
}
