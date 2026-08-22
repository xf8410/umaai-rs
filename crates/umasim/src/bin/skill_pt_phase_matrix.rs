//! 分年份动态技能 PT 权重实验：相同 seed 配对基准与候选。

use std::{env, fs::File, io::Write, sync::Mutex};

use anyhow::{Context, Result};
use rand::prelude::StdRng;
use umasim::{
    bench,
    game::{Game, InheritInfo, Trainer, ramen::{RamenAction, RamenGame, RamenStage}},
    gamedata::{EventChoice, EventData, GAMECONSTANTS, init_global_with_config},
    global,
    trainer::{LocalRamenTrainer, LoggingTrainer, RamenHandwrittenTrainer},
    utils::{get_workspace_root, load_game_config}
};

const BASE_SEED: u64 = 61444;
const UMA: u32 = 102601;
const DECK: [u32; 6] = [302424, 302894, 303044, 302924, 303024, 303054];
const INHERIT: InheritInfo = InheritInfo { blue_count: [15, 0, 0, 0, 3], extra_count: [10, 10, 20, 20, 20, 40] };

/// 仅按年份切换已验证的 LocalRamenTrainer；不引入卡组或支援卡分类。
struct PhaseTrainer {
    years: [LocalRamenTrainer; 3],
    last: Mutex<Option<usize>>,
}
impl PhaseTrainer {
    fn new(pt: [u32; 3], sac: u32) -> Result<Self> {
        Ok(Self {
            years: [
                LocalRamenTrainer::matrix_variant(&format!("pt{}-sac{sac}-long-fail0", pt[0]))?,
                LocalRamenTrainer::matrix_variant(&format!("pt{}-sac{sac}-long-fail0", pt[1]))?,
                LocalRamenTrainer::matrix_variant(&format!("pt{}-sac{sac}-long-fail0", pt[2]))?,
            ],
            last: Mutex::new(None),
        })
    }
    fn year(game: &RamenGame) -> usize { if game.turn() < 24 { 0 } else if game.turn() < 48 { 1 } else { 2 } }
    fn active(&self, game: &RamenGame) -> &LocalRamenTrainer { &self.years[Self::year(game)] }
}
impl Trainer<RamenGame> for PhaseTrainer {
    fn select_action(&self, game: &RamenGame, actions: &[RamenAction], rng: &mut StdRng) -> Result<usize> {
        let y=Self::year(game); *self.last.lock().unwrap()=Some(y); self.years[y].select_action(game, actions, rng)
    }
    fn select_choice(&self, game: &RamenGame, choices: &[Vec<EventChoice>], rng: &mut StdRng) -> Result<usize> {
        let y=Self::year(game); *self.last.lock().unwrap()=Some(y); self.years[y].select_choice(game, choices, rng)
    }
    fn select_event_choice(&self, game: &RamenGame, event: &EventData, choices: &[Vec<EventChoice>], rng: &mut StdRng) -> Result<usize> {
        let y=Self::year(game); *self.last.lock().unwrap()=Some(y); self.years[y].select_event_choice(game, event, choices, rng)
    }
    fn last_breakdown(&self) -> Option<String> {
        let y=(*self.last.lock().ok()?)?; self.years[y].last_breakdown()
    }
}

fn status_score(s: &[i32;5])->i32 { let c=global!(GAMECONSTANTS); s.iter().map(|&v| c.five_status_final_score[(v.max(0) as usize).min(c.five_status_final_score.len()-1)]).sum() }
fn run<T:Trainer<RamenGame>>(t:T,i:u64)->Result<bench::GameOutcome>{bench::run_seeded(UMA,&DECK,&INHERIT,BASE_SEED,i,&LoggingTrainer::new(t,i))}
fn main()->Result<()> {
    let variant=env::var("VARIANT").context("缺少 VARIANT")?;
    let pt=[env::var("PT1")?.parse()?,env::var("PT2")?.parse()?,env::var("PT3")?.parse()?];
    let sac=env::var("SAC")?.parse()?;
    let shard:u64=env::var("SHARD").unwrap_or_else(|_|"0".into()).parse()?;
    let runs:u64=env::var("RUNS_PER_SHARD").unwrap_or_else(|_|"100".into()).parse()?;
    std::env::set_current_dir(get_workspace_root()?)?; init_global_with_config(&load_game_config()?)?;
    let mut f=File::create("matrix-result.csv")?;
    writeln!(f,"variant,shard,run_idx,a_score,b_score,a_skill_pt,b_skill_pt,a_status_score,b_status_score,a_status_sum,b_status_sum")?;
    for off in 0..runs { let i=shard*runs+off; let a=run(RamenHandwrittenTrainer::new(),i)?; let b=run(PhaseTrainer::new(pt,sac)?,i)?;
        writeln!(f,"{variant},{shard},{i},{},{},{},{},{},{},{},{}",a.score,b.score,a.skill_pt,b.skill_pt,status_score(&a.five_status),status_score(&b.five_status),a.five_status.iter().sum::<i32>(),b.five_status.iter().sum::<i32>())?; }
    Ok(())
}
