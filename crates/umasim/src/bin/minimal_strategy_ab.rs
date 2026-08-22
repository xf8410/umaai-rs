//! 最小策略 A/B：同一马娘、卡组、继承和固定 seed 比较上游与本地扩展策略。

use anyhow::{Result, ensure};
use umasim::{
    bench::{self, GameOutcome},
    game::{InheritInfo, ramen::rules::calc_ramen_pt_gain},
    gamedata::init_global_with_config,
    output::decision_log::DecisionLogRow,
    trainer::{LocalRamenTrainer, LoggingTrainer, RamenHandwrittenTrainer},
    utils::{get_workspace_root, load_game_config}
};

const RUNS: usize = 10;
const BASE_SEED: u64 = 61444;
const UMA: u32 = 102601;
const DECK: [u32; 6] = [302424, 302894, 303044, 302924, 303024, 303054];
const INHERIT: InheritInfo = InheritInfo {
    blue_count: [15, 0, 0, 0, 3],
    extra_count: [10, 10, 20, 20, 20, 40]
};

/// `RamenState::scenario_pt` 和 `eat_count` 都会在年度 RMJ 结算后归零，不能在整局
/// 结束时直接当累计值读取。决策日志完整记录了每次 RamenSelect 的实际选择；据此按
/// 年份和当年吃面序号，用游戏规则本身重建整局累计 PT 与吃面次数。
fn apply_cumulative_ramen_metrics(outcome: &mut GameOutcome, rows: &[DecisionLogRow]) -> Result<()> {
    let mut yearly_eat = [0_i32; 3];
    let mut total_pt = 0_i32;

    for row in rows {
        if row.stage != "RamenSelect" || !row.action_desc.starts_with("吃面/") {
            continue;
        }
        let year_idx = if row.turn < 24 {
            0
        } else if row.turn < 48 {
            1
        } else {
            2
        };
        total_pt += calc_ramen_pt_gain(year_idx, yearly_eat[year_idx])?;
        yearly_eat[year_idx] += 1;
    }

    let total_eat = yearly_eat.iter().sum();
    ensure!(total_eat > 0, "seed={} 未记录到任何吃面决策，累计指标无效", outcome.seed);
    ensure!(total_pt > 0, "seed={} 累计剧本 PT 异常为 0", outcome.seed);
    outcome.scenario_pt = total_pt;
    outcome.eat_count = total_eat;
    Ok(())
}

fn print_summary(name: &str, outcomes: &[GameOutcome]) {
    let scores = outcomes.iter().map(|x| x.score as f64).collect::<Vec<_>>();
    let stats = bench::summarize(&scores);
    let rmj = outcomes.iter().map(|x| x.rmj_ok as f64).sum::<f64>() / outcomes.len() as f64;
    let pt = outcomes.iter().map(|x| x.scenario_pt as f64).sum::<f64>() / outcomes.len() as f64;
    let eat = outcomes.iter().map(|x| x.eat_count as f64).sum::<f64>() / outcomes.len() as f64;
    println!(
        "RESULT {name}: n={} mean={:.0} median={:.0} min={:.0} max={:.0} std={:.0} RMJ_success_years={:.2}/3 cumulative_PT={:.0} total_eat={:.1}",
        outcomes.len(), stats.mean, stats.median, stats.min, stats.max, stats.std, rmj, pt, eat
    );
}

fn main() -> Result<()> {
    let root = get_workspace_root()?;
    std::env::set_current_dir(root)?;
    init_global_with_config(&load_game_config()?)?;

    let mut upstream_results = Vec::with_capacity(RUNS);
    let mut local_results = Vec::with_capacity(RUNS);

    println!("Minimal A/B: {RUNS} games/strategy, seeds {BASE_SEED}..{}", BASE_SEED + RUNS as u64 - 1);
    for i in 0..RUNS {
        let seed = BASE_SEED + i as u64;
        // 保留内存中的决策记录用于重建跨年度累计指标；不把数千条逐回合记录打印到控制台。
        let upstream = LoggingTrainer::new(RamenHandwrittenTrainer::new(), seed);
        let local = LoggingTrainer::new(LocalRamenTrainer::new(), seed);

        let mut a = bench::run_seeded(UMA, &DECK, &INHERIT, seed, &upstream)?;
        let mut b = bench::run_seeded(UMA, &DECK, &INHERIT, seed, &local)?;
        apply_cumulative_ramen_metrics(&mut a, &upstream.take_records().rows)?;
        apply_cumulative_ramen_metrics(&mut b, &local.take_records().rows)?;

        println!(
            "seed={seed} upstream_score={} local_score={} delta={} RMJ_upstream={}/3 RMJ_local={}/3 PT_upstream={} PT_local={} eat_upstream={} eat_local={}",
            a.score,
            b.score,
            b.score - a.score,
            a.rmj_ok,
            b.rmj_ok,
            a.scenario_pt,
            b.scenario_pt,
            a.eat_count,
            b.eat_count
        );
        upstream_results.push(a);
        local_results.push(b);
    }

    print_summary("upstream", &upstream_results);
    print_summary("local", &local_results);
    let mean_a = upstream_results.iter().map(|x| x.score as f64).sum::<f64>() / RUNS as f64;
    let mean_b = local_results.iter().map(|x| x.score as f64).sum::<f64>() / RUNS as f64;
    println!("DELTA local-upstream={:.0}", mean_b - mean_a);
    Ok(())
}
