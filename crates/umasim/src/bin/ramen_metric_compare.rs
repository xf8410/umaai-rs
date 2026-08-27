//! 配对验证 ramen 手写策略：同一批规则种子比较 baseline 与 candidate。
//!
//! 卡组通过环境变量配置（各类型代表卡 + 固定友人卡），与其他矩阵工具的
//! 配卡口径一致：
//! - `DECK_COUNTS` 五位数字「速耐力根智」各类型张数（默认 `21002`＝2速1耐2智）；
//! - `DECK_LABEL` 自由文本标签（默认跟随 counts）。
//! `SIDE` 只作输出标签；真正的配对比较由工作流分别在本仓库（candidate）
//! 与上游基线 checkout 中运行本程序后完成。

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

/// 解析五位数字卡组规格「速耐力根智」，例如 `21002`。
fn parse_deck_counts(spec: &str) -> Result<[usize; 5]> {
    let bytes = spec.as_bytes();
    ensure!(
        bytes.len() == 5 && bytes.iter().all(u8::is_ascii_digit),
        "DECK_COUNTS 必须是 5 位数字（速耐力根智）: {spec}"
    );
    let counts: [usize; 5] = bytes
        .iter()
        .map(|b| (b - b'0') as usize)
        .collect::<Vec<_>>()
        .try_into()
        .expect("长度已校验为 5");
    let total: usize = counts.iter().sum();
    ensure!(total == 5, "支援卡总数必须为 5（另有 1 张固定友人卡）: {spec} 合计 {total}");
    Ok(counts)
}

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

    let counts = parse_deck_counts(&env::var("DECK_COUNTS").unwrap_or_else(|_| "21002".to_string()))?;
    let label = env::var("DECK_LABEL").unwrap_or_else(|_| format!("{counts:?}"));
    let composition = DeckComposition {
        counts,
        name: label.clone()
    };
    let reps = bench::select_representatives(&CardPickOpts::default())?;
    let deck = composition.build_deck(&reps.picked, FRIEND)?;

    println!("composition={label} counts={counts:?} runs={runs}");

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
