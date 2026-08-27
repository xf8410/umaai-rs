//! 配对验证 ramen 手写策略：同一批规则种子比较 baseline 与 candidate。
//!
//! 卡组固定为 2速1耐2智（`counts=[2,1,0,0,2]`，各类型代表卡 + 固定友人卡），
//! 与其他矩阵工具的配卡口径一致。`SIDE` 只作输出标签；真正的配对比较由
//! 工作流分别在本仓库（candidate）与上游基线 checkout 中运行本程序后完成。

use std::{env, fs::File, io::Write};

use anyhow::{Context, Result, ensure};
use umasim::{
    bench::{self, CardPickOpts, DeckComposition},
    game::InheritInfo,
    gamedata::{GAMECONSTANTS, init_global_with_config},
    global,
    trainer::{LoggingTrainer, RecommendedRamenTrainer},
    utils::{get_workspace_root, load_game_config}
};

const UMA: u32 = 102601;
const FRIEND: u32 = 303054;
const BASE_SEED: u64 = 995_100;
const INHERIT: InheritInfo = InheritInfo {
    blue_count: [15, 0, 0, 0, 3],
    extra_count: [10, 10, 20, 20, 20, 40]
};

fn metric(outcomes: &[bench::GameOutcome]) -> (f64, f64, f64) {
    let n = outcomes.len().max(1) as f64;
    let score = outcomes.iter().map(|x| x.score as f64).sum::<f64>() / n;
    let attribute_score = outcomes
        .iter()
        .map(|x| {
            x.five_status
                .iter()
                .map(|&v| {
                    let idx =
                        (v.max(0) as usize).min(global!(GAMECONSTANTS).five_status_final_score.len() - 1);
                    global!(GAMECONSTANTS).five_status_final_score[idx] as f64
                })
                .sum::<f64>()
        })
        .sum::<f64>()
        / n;
    let skill_pt = outcomes.iter().map(|x| x.skill_pt as f64).sum::<f64>() / n;
    (score, attribute_score, skill_pt)
}

fn main() -> Result<()> {
    let runs: u64 = env::var("RUNS")
        .unwrap_or_else(|_| "300".to_string())
        .parse()
        .context("RUNS 必须是整数")?;
    ensure!(runs > 0, "RUNS 必须大于 0");

    std::env::set_current_dir(get_workspace_root()?)?;
    init_global_with_config(&load_game_config()?)?;

    let composition =
        DeckComposition { counts: [2, 1, 0, 0, 2], name: "2速1耐2智".to_string() };
    let reps = bench::select_representatives(&CardPickOpts::default())?;
    let deck = composition.build_deck(&reps.picked, FRIEND)?;

    let mut outcomes = Vec::with_capacity(runs as usize);
    for run_idx in 0..runs {
        let trainer = LoggingTrainer::new(RecommendedRamenTrainer::new(), run_idx);
        let outcome = bench::run_seeded(UMA, &deck, &INHERIT, BASE_SEED, run_idx, &trainer)
            .with_context(|| format!("run failed at run_idx={run_idx}"))?;
        outcomes.push(outcome);
    }

    let (score, attribute_score, skill_pt) = metric(&outcomes);
    let side = env::var("SIDE").unwrap_or_else(|_| "unknown".to_string());
    let path = env::var("OUT").unwrap_or_else(|_| "ramen-metrics.txt".to_string());
    let mut file = File::create(path)?;
    writeln!(file, "side={side}")?;
    writeln!(file, "composition={}", composition.name())?;
    writeln!(file, "runs={runs}")?;
    writeln!(file, "score_mean={score:.9}")?;
    writeln!(file, "attribute_score_mean={attribute_score:.9}")?;
    writeln!(file, "skill_pt_mean={skill_pt:.9}")?;
    Ok(())
}
