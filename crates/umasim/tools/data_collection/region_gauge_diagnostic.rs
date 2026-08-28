//! 诀窍流转观测采集：统计「实际使用策略」(`RecommendedRamenTrainer`) 下，
//! 每年的友情训练回合数 / 吃面次数 / 诀窍获得 / 诀窍溢出。
//!
//! 用途：为整局对比与地区选择质量评估提供实际运行数据（配合 `region_matrix`
//! 的固定地区 A/B 整局对比使用；诀窍模拟类打分指标已弃用，见 issues.md）。
//! 纯观测，不改变任何模拟数值。

use std::env;

use anyhow::Result;
use umasim::{
    bench,
    game::InheritInfo,
    gamedata::init_global_with_config,
    trainer::{LoggingTrainer, RecommendedRamenTrainer},
    utils::{get_workspace_root, load_game_config}
};

/// 基准种子（与 `ramen_low_score_diagnostic` 一致）。
const BASE_SEED: u64 = 61_444;
const UMA: u32 = 102_601;
const DECK: [u32; 6] = [302424, 302894, 303044, 302924, 303024, 303054];
const INHERIT: InheritInfo = InheritInfo {
    blue_count: [15, 0, 0, 0, 3],
    extra_count: [10, 10, 20, 20, 20, 40]
};

/// 每年诀窍填充回合数近似（第 1 年 turn 2-23 ≈ 22，第 2/3 年 ≈ 24），用于换算占比。
const TURNS_PER_YEAR: [f64; 3] = [22.0, 24.0, 24.0];

fn main() -> Result<()> {
    std::env::set_current_dir(get_workspace_root()?)?;
    init_global_with_config(&load_game_config()?)?;

    let runs: u64 = env::var("DIAG_RUNS").unwrap_or_else(|_| "30".into()).parse()?;

    let mut friend_sum = [0f64; 3];
    let mut eat_sum = [0f64; 3];
    let mut gain_sum = [0f64; 3];
    let mut overflow_sum = [0f64; 3];
    let mut score_sum = 0f64;

    for run_idx in 0..runs {
        let trainer = LoggingTrainer::new(RecommendedRamenTrainer::new(), run_idx);
        let outcome = bench::run_seeded(UMA, &DECK, &INHERIT, BASE_SEED, run_idx, &trainer)?;
        for y in 0..3 {
            friend_sum[y] += outcome.yearly_friend_turns[y] as f64;
            eat_sum[y] += outcome.yearly_eat_count[y] as f64;
            gain_sum[y] += outcome.yearly_gauge_gain[y] as f64;
            overflow_sum[y] += outcome.yearly_gauge_overflow[y] as f64;
        }
        score_sum += outcome.score as f64;
    }

    let n = runs as f64;
    println!("=== 地区诀窍流转观测（{runs} 局均值, seed={BASE_SEED}, 卡组={DECK:?}）===");
    println!("平均评分: {:.0}", score_sum / n);
    println!(
        "{:<14}{:>10}{:>10}{:>10}",
        "指标\\年份", "第1年", "第2年", "第3年"
    );
    let row = |name: &str, v: &[f64; 3]| {
        println!(
            "{:<14}{:>10.1}{:>10.1}{:>10.1}",
            name, v[0] / n, v[1] / n, v[2] / n
        );
    };
    row("友训回合数", &friend_sum);
    row("吃面次数", &eat_sum);
    row("诀窍获得", &gain_sum);
    row("诀窍溢出", &overflow_sum);
    for y in 0..3 {
        let rate = friend_sum[y] / (n * TURNS_PER_YEAR[y]);
        println!(
            "第{}年友训占比: {:.1}% (={:.1}/{:.0} 回合)",
            y + 1,
            rate * 100.0,
            friend_sum[y] / n,
            TURNS_PER_YEAR[y]
        );
    }
    Ok(())
}