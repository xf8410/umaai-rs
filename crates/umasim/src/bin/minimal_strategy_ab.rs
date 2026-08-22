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
/// 年份和当年吃面序号，用游戏规则本身重建整局累计 RMJ 剧本 PT 与吃面次数。
///
/// 注意：这里的 RMJ 剧本 PT 是“吃面获得、用于年度 RMJ 判定和剧本常驻加成”的点数，
/// 与训练等途径获得、最终用于购买技能的 `Uma::skill_pt` 完全不是同一种资源。
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
    ensure!(total_pt > 0, "seed={} 累计 RMJ 剧本 PT 异常为 0", outcome.seed);
    outcome.scenario_pt = total_pt;
    outcome.eat_count = total_eat;
    Ok(())
}

/// 输出所有机器字段的完整说明，避免把不同 PT、单局值和多局平均值混为一谈。
fn print_legend() {
    println!("===== 字段说明 / LEGEND =====");
    println!("seed: 随机种子；同一 seed 下 upstream/local 使用相同初始随机条件进行配对比较。");
    println!("upstream/local: upstream=原上游手写策略；local=本地修改后的策略。");
    println!("*_score: 单局最终育成评分；delta=local_score-upstream_score，正数表示本地策略该局更高。");
    println!("RMJ_*: 单局三个年度 RMJ 中成功的年度数；3/3 表示三个年度全部成功。");
    println!("RMJ_cumulative_scenario_PT_*: 单局三个年度吃面所得 RMJ 剧本 PT 的累计和；用于年度 RMJ 判定/剧本加成，每年结算后游戏状态会清零。");
    println!("skill_PT_*: 单局结束时技能点；来自训练/比赛/事件等，用于购买技能；与 RMJ 剧本 PT 不是同一种资源。");
    println!("ramen_eaten_*: 单局三个年度合计吃面碗数；不是当年剩余值，也不是平均值。");
    println!("n: 每个策略完成的局数。");
    println!("mean/median/min/max/std: 最终育成评分的平均值/中位数/最小值/最大值/总体标准差。");
    println!("avg_RMJ_success_years: 每局 RMJ 成功年度数的平均值，满值为 3。");
    println!("avg_RMJ_cumulative_scenario_PT: 每局累计 RMJ 剧本 PT 的平均值。");
    println!("avg_skill_PT: 每局结束技能点的平均值。");
    println!("avg_ramen_eaten_per_game: 每局三个年度合计吃面碗数的平均值；例如 25.7 表示平均每局吃 25.7 碗。");
    println!("DELTA local_mean_score-upstream_mean_score: 两策略平均最终评分之差；正数表示本地策略平均分更高。");
    println!("=============================");
}

fn print_summary(name: &str, outcomes: &[GameOutcome]) {
    let scores = outcomes.iter().map(|x| x.score as f64).collect::<Vec<_>>();
    let stats = bench::summarize(&scores);
    let rmj = outcomes.iter().map(|x| x.rmj_ok as f64).sum::<f64>() / outcomes.len() as f64;
    let rmj_pt = outcomes.iter().map(|x| x.scenario_pt as f64).sum::<f64>() / outcomes.len() as f64;
    let skill_pt = outcomes.iter().map(|x| x.skill_pt as f64).sum::<f64>() / outcomes.len() as f64;
    let eat = outcomes.iter().map(|x| x.eat_count as f64).sum::<f64>() / outcomes.len() as f64;
    println!(
        "RESULT {name}: n={} mean={:.0} median={:.0} min={:.0} max={:.0} std={:.0} avg_RMJ_success_years={:.2}/3 avg_RMJ_cumulative_scenario_PT={:.0} avg_skill_PT={:.0} avg_ramen_eaten_per_game={:.1}",
        outcomes.len(), stats.mean, stats.median, stats.min, stats.max, stats.std, rmj, rmj_pt, skill_pt, eat
    );
}

fn main() -> Result<()> {
    let root = get_workspace_root()?;
    std::env::set_current_dir(root)?;
    init_global_with_config(&load_game_config()?)?;

    print_legend();

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
            "seed={seed} upstream_score={} local_score={} delta={} RMJ_upstream={}/3 RMJ_local={}/3 RMJ_cumulative_scenario_PT_upstream={} RMJ_cumulative_scenario_PT_local={} skill_PT_upstream={} skill_PT_local={} ramen_eaten_upstream={} ramen_eaten_local={}",
            a.score,
            b.score,
            b.score - a.score,
            a.rmj_ok,
            b.rmj_ok,
            a.scenario_pt,
            b.scenario_pt,
            a.skill_pt,
            b.skill_pt,
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
    println!("DELTA local_mean_score-upstream_mean_score={:.0}", mean_b - mean_a);
    Ok(())
}
