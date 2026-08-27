//! 配对验证 ramen 手写策略：同一批规则种子比较 baseline 与 candidate。
use std::{env, fs::File, io::Write};

use anyhow::{Context, Result, ensure};
use umasim::{
    bench,
    game::InheritInfo,
    gamedata::{GAMECONSTANTS, init_global_with_config},
    global,
    trainer::{LoggingTrainer, RecommendedRamenTrainer},
    utils::{get_workspace_root, load_game_config}
};

const UMA: u32 = 102601;
const FRIEND: u32 = 303054;
const BASE_SEED: u64 = 995_100;
const DECK: [u32; 6] = [302424, 302894, 303044, 302924, 303024, 303054];
const INHERIT: InheritInfo = InheritInfo {
    blue_count: [15, 0, 0, 0, 3],
    extra_count: [10, 10, 20, 20, 20, 40]
};

fn metric(outcomes: &[bench::GameOutcome]) -> (f64, f64, f64, f64) {
    let n = outcomes.len().max(1) as f64;
    let score = outcomes.iter().map(|x| x.score as f64).sum::<f64>() / n;
    let attr_raw = outcomes
        .iter()
        .map(|x| x.five_status.iter().map(|&v| v as f64).sum::<f64>())
        .sum::<f64>()
        / n;
    let attr_score = outcomes
        .iter()
        .map(|x| {
            x.five_status
                .iter()
                .map(|&v| {
                    let idx = (v.max(0) as usize).min(global!(GAMECONSTANTS).five_status_final_score.len() - 1);
                    global!(GAMECONSTANTS).five_status_final_score[idx] as f64
                })
                .sum::<f64>()
        })
        .sum::<f64>()
        / n;
    let pt = outcomes.iter().map(|x| x.skill_pt as f64).sum::<f64>() / n;
    (score, attr_raw, attr_score, pt)
}

fn main() -> Result<()> {
    let runs: u64 = env::var("RUNS").unwrap_or_else(|_| "300".to_string()).parse()?;
    ensure!(runs > 0, "RUNS must be > 0");
    std::env::set_current_dir(get_workspace_root()?)?;
    init_global_with_config(&load_game_config()?)?;

    let mut baseline = Vec::with_capacity(runs as usize);
    let mut candidate = Vec::with_capacity(runs as usize);
    for run_idx in 0..runs {
        let base_trainer = LoggingTrainer::new(RecommendedRamenTrainer::new(), run_idx);
        let candidate_trainer = LoggingTrainer::new(RecommendedRamenTrainer::new(), run_idx);
        // This binary is run once in the baseline checkout and once in the candidate checkout.
        // The workflow supplies the checkout label; the actual paired comparison is performed
        // by the surrounding script, which stores each side's JSON metrics.
        let outcome = bench::run_seeded(UMA, &DECK, &INHERIT, BASE_SEED, run_idx, &base_trainer)
            .with_context(|| format!("baseline/candidate run failed at {run_idx}"))?;
        if env::var("SIDE").as_deref() == Ok("baseline") {
            baseline.push(outcome);
        } else {
            candidate.push(outcome);
        }
        let _ = candidate_trainer;
    }
    let values = if env::var("SIDE").as_deref() == Ok("baseline") {
        metric(&baseline)
    } else {
        metric(&candidate)
    };
    let path = env::var("OUT").unwrap_or_else(|_| "ramen-metrics.txt".to_string());
    let mut file = File::create(path)?;
    writeln!(file, "runs={runs}")?;
    writeln!(file, "score_mean={:.9}", values.0)?;
    writeln!(file, "attribute_raw_mean={:.9}", values.1)?;
    writeln!(file, "attribute_score_mean={:.9}", values.2)?;
    writeln!(file, "skill_pt_mean={:.9}", values.3)?;
    Ok(())
}
