//! 单个矩阵单元：同一批固定种子配对运行基准手写策略和一个候选策略。

use std::{env, fs::File, io::Write};

use anyhow::{Context, Result};
use umasim::{
    bench,
    game::{InheritInfo, Trainer, ramen::RamenGame},
    gamedata::{GAMECONSTANTS, init_global_with_config},
    global,
    trainer::{LocalRamenTrainer, RamenHandwrittenTrainer},
    utils::{get_workspace_root, load_game_config}
};

const BASE_SEED: u64 = 61444;
const UMA: u32 = 102601;
const DECK: [u32; 6] = [302424, 302894, 303044, 302924, 303024, 303054];
const INHERIT: InheritInfo = InheritInfo {
    blue_count: [15, 0, 0, 0, 3],
    extra_count: [10, 10, 20, 20, 20, 40]
};

fn status_score(status: &[i32; 5]) -> i32 {
    let cons = global!(GAMECONSTANTS);
    status
        .iter()
        .map(|&value| cons.five_status_final_score[value.max(0) as usize])
        .sum()
}

fn run<T: Trainer<RamenGame>>(trainer: &T, run_idx: u64) -> Result<bench::GameOutcome> {
    let wrapped = umasim::trainer::LoggingTrainer::new(trainer, run_idx);
    bench::run_seeded(UMA, &DECK, &INHERIT, BASE_SEED, run_idx, &wrapped)
}

fn main() -> Result<()> {
    let variant = env::var("VARIANT").context("缺少 VARIANT")?;
    let shard: u64 = env::var("SHARD").unwrap_or_else(|_| "0".into()).parse()?;
    let runs: u64 = env::var("RUNS_PER_SHARD").unwrap_or_else(|_| "100".into()).parse()?;
    let workspace = get_workspace_root()?;
    std::env::set_current_dir(workspace)?;
    init_global_with_config(&load_game_config()?)?;

    let baseline = RamenHandwrittenTrainer::new();
    let candidate = LocalRamenTrainer::matrix_variant(&variant)?;
    let mut output = File::create("matrix-result.csv")?;
    writeln!(output, "variant,shard,run_idx,a_score,b_score,a_skill_pt,b_skill_pt,a_status_score,b_status_score,a_status_sum,b_status_sum")?;

    for offset in 0..runs {
        let run_idx = shard * runs + offset;
        let a = run(&baseline, run_idx)?;
        let b = run(&candidate, run_idx)?;
        let a_sum: i32 = a.five_status.iter().sum();
        let b_sum: i32 = b.five_status.iter().sum();
        writeln!(
            output,
            "{variant},{shard},{run_idx},{},{},{},{},{},{},{},{}",
            a.score,
            b.score,
            a.skill_pt,
            b.skill_pt,
            status_score(&a.five_status),
            status_score(&b.five_status),
            a_sum,
            b_sum
        )?;
    }
    Ok(())
}
