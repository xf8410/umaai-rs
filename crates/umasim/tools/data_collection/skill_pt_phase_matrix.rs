//! 分年份动态技能 PT 权重与结构手写逻辑实验：相同 seed 配对基准与候选。

use std::{env, fs::File, io::Write, sync::Mutex};

use anyhow::{Context, Result};
use rand::prelude::StdRng;
use umasim::{
    bench,
    game::{
        Game,
        InheritInfo,
        Trainer,
        ramen::{RamenAction, RamenGame}
    },
    gamedata::{EventChoice, EventData, GAMECONSTANTS, init_global_with_config},
    global,
    trainer::{LocalRamenTrainer, LoggingTrainer, RamenHandwrittenTrainer},
    utils::{get_workspace_root, load_game_config}
};
const BASE_SEED: u64 = 61444;
const UMA: u32 = 102601;
const DECK: [u32; 6] = [302424, 302894, 303044, 302924, 303024, 303054];
const INHERIT: InheritInfo = InheritInfo {
    blue_count: [15, 0, 0, 0, 3],
    extra_count: [10, 10, 20, 20, 20, 40]
};
struct PhaseTrainer {
    years: [LocalRamenTrainer; 3],
    last: Mutex<Option<usize>>
}
impl PhaseTrainer {
    fn new(pt: [u32; 3], sac: u32, extra: &str) -> Result<Self> {
        let suffix = if extra.is_empty() {
            String::new()
        } else {
            format!("-{extra}")
        };
        let make = |p| LocalRamenTrainer::matrix_variant(&format!("pt{p}-sac{sac}-long-fail0{suffix}"));
        Ok(Self {
            years: [make(pt[0])?, make(pt[1])?, make(pt[2])?],
            last: Mutex::new(None)
        })
    }
    fn year(g: &RamenGame) -> usize {
        if g.turn() < 24 {
            0
        } else if g.turn() < 48 {
            1
        } else {
            2
        }
    }
}
impl Trainer<RamenGame> for PhaseTrainer {
    fn select_action(&self, g: &RamenGame, a: &[RamenAction], r: &mut StdRng) -> Result<usize> {
        let y = Self::year(g);
        *self.last.lock().unwrap() = Some(y);
        self.years[y].select_action(g, a, r)
    }
    fn select_choice(&self, g: &RamenGame, c: &[Vec<EventChoice>], r: &mut StdRng) -> Result<usize> {
        let y = Self::year(g);
        *self.last.lock().unwrap() = Some(y);
        self.years[y].select_choice(g, c, r)
    }
    fn select_event_choice(
        &self, g: &RamenGame, e: &EventData, c: &[Vec<EventChoice>], r: &mut StdRng
    ) -> Result<usize> {
        let y = Self::year(g);
        *self.last.lock().unwrap() = Some(y);
        self.years[y].select_event_choice(g, e, c, r)
    }
    fn last_breakdown(&self) -> Option<String> {
        let y = (*self.last.lock().ok()?)?;
        self.years[y].last_breakdown()
    }
}
fn status_score(s: &[i32; 5]) -> i32 {
    let c = global!(GAMECONSTANTS);
    s.iter()
        .map(|&v| c.five_status_final_score[(v.max(0) as usize).min(c.five_status_final_score.len() - 1)])
        .sum()
}
fn run<T: Trainer<RamenGame>>(t: T, i: u64) -> Result<bench::GameOutcome> {
    bench::run_seeded(UMA, &DECK, &INHERIT, BASE_SEED, i, &LoggingTrainer::new(t, i))
}
fn main() -> Result<()> {
    let variant = env::var("VARIANT").context("缺少 VARIANT")?;
    let pt = [
        env::var("PT1")?.parse()?,
        env::var("PT2")?.parse()?,
        env::var("PT3")?.parse()?
    ];
    let sac = env::var("SAC")?.parse()?;
    let extra = env::var("STRUCTURE").unwrap_or_default();
    let shard: u64 = env::var("SHARD").unwrap_or_else(|_| "0".into()).parse()?;
    let runs: u64 = env::var("RUNS_PER_SHARD").unwrap_or_else(|_| "100".into()).parse()?;
    std::env::set_current_dir(get_workspace_root()?)?;
    init_global_with_config(&load_game_config()?)?;
    // Parse and construct all three yearly policies even for a zero-run smoke test. This gives CI a
    // targeted parser check without compiling every unrelated #[cfg(test)] module in the library.
    let validation = PhaseTrainer::new(pt, sac, &extra).context("分阶段策略参数验证失败")?;
    drop(validation);
    let mut f = File::create("matrix-result.csv")?;
    writeln!(
        f,
        "variant,shard,run_idx,a_score,b_score,a_skill_pt,b_skill_pt,a_status_score,b_status_score,a_status_sum,b_status_sum"
    )?;
    for off in 0..runs {
        let i = shard * runs + off;
        let a = run(RamenHandwrittenTrainer::new(), i)?;
        let b = run(PhaseTrainer::new(pt, sac, &extra)?, i)?;
        writeln!(
            f,
            "{variant},{shard},{i},{},{},{},{},{},{},{},{}",
            a.score,
            b.score,
            a.skill_pt,
            b.skill_pt,
            status_score(&a.five_status),
            status_score(&b.five_status),
            a.five_status.iter().sum::<i32>(),
            b.five_status.iter().sum::<i32>()
        )?;
    }
    Ok(())
}
