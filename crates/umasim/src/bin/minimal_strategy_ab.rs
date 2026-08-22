//! 策略 A/B：同一马娘、卡组、继承和固定 seed 比较原策略与本地策略。

use anyhow::{Result, ensure};
use umasim::{
    bench::{self, GameOutcome},
    game::{InheritInfo, ramen::rules::calc_ramen_pt_gain},
    gamedata::init_global_with_config,
    output::decision_log::DecisionLogRow,
    trainer::{LocalRamenTrainer, LoggingTrainer, RamenHandwrittenTrainer},
    utils::{get_workspace_root, load_game_config}
};

const RUNS: usize = 1000;
const BASE_SEED: u64 = 61444;
const UMA: u32 = 102601;
const DECK: [u32; 6] = [302424, 302894, 303044, 302924, 303024, 303054];
const INHERIT: InheritInfo = InheritInfo {
    blue_count: [15, 0, 0, 0, 3],
    extra_count: [10, 10, 20, 20, 20, 40]
};

/// `scenario_pt` 和 `eat_count` 会在年度 RMJ 结算后归零，因此根据每次真实吃面
/// 决策重建三个年度合计的 RMJ 剧本点和吃面碗数。
/// RMJ 剧本点用于年度 RMJ 判定/剧本加成；`skill_pt` 是购买技能用的技能点。
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
    ensure!(total_pt > 0, "seed={} 累计 RMJ 剧本点异常为 0", outcome.seed);
    outcome.scenario_pt = total_pt;
    outcome.eat_count = total_eat;
    Ok(())
}

fn print_summary(label: &str, outcomes: &[GameOutcome]) {
    let scores = outcomes.iter().map(|x| x.score as f64).collect::<Vec<_>>();
    let stats = bench::summarize(&scores);
    let rmj = outcomes.iter().map(|x| x.rmj_ok as f64).sum::<f64>() / outcomes.len() as f64;
    let rmj_pt = outcomes.iter().map(|x| x.scenario_pt as f64).sum::<f64>() / outcomes.len() as f64;
    let skill_pt = outcomes.iter().map(|x| x.skill_pt as f64).sum::<f64>() / outcomes.len() as f64;
    let eaten = outcomes.iter().map(|x| x.eat_count as f64).sum::<f64>() / outcomes.len() as f64;
    println!(
        "RESULT {label}: 局数={} 平均评分={:.0} 中位评分={:.0} 最低评分={:.0} 最高评分={:.0} 评分总体标准差={:.0} 平均RMJ成功={:.2}/3年 平均累计RMJ剧本点={:.0} 平均最终技能点={:.0} 平均每局吃面={:.1}碗",
        outcomes.len(), stats.mean, stats.median, stats.min, stats.max, stats.std, rmj, rmj_pt, skill_pt, eaten
    );
}

fn main() -> Result<()> {
    let root = get_workspace_root()?;
    std::env::set_current_dir(root)?;
    init_global_with_config(&load_game_config()?)?;

    println!("A=原上游手写策略；B=本地修改策略；每行比较同一个随机种子下的 A 与 B。");
    println!("RMJ剧本点=吃面获得，用于年度RMJ判定和剧本加成；技能点=训练/比赛/事件等获得，用于购买技能。两者不是同一种点数。");
    println!("RMJ成功=三个年度中成功的年度数；吃面=一整局三个年度合计吃面碗数。");

    let mut a_results = Vec::with_capacity(RUNS);
    let mut b_results = Vec::with_capacity(RUNS);

    println!("开始 A/B：每套策略 {RUNS} 局，随机种子 {BASE_SEED}..{}", BASE_SEED + RUNS as u64 - 1);
    for i in 0..RUNS {
        let seed = BASE_SEED + i as u64;
        let trainer_a = LoggingTrainer::new(RamenHandwrittenTrainer::new(), seed);
        let trainer_b = LoggingTrainer::new(LocalRamenTrainer::new(), seed);

        let mut a = bench::run_seeded(UMA, &DECK, &INHERIT, seed, &trainer_a)?;
        let mut b = bench::run_seeded(UMA, &DECK, &INHERIT, seed, &trainer_b)?;
        apply_cumulative_ramen_metrics(&mut a, &trainer_a.take_records().rows)?;
        apply_cumulative_ramen_metrics(&mut b, &trainer_b.take_records().rows)?;

        println!(
            "seed={seed} | A(原策略)[评分={}，RMJ成功={}/3年，累计RMJ剧本点={}，最终技能点={}，全局吃面={}碗] | B(本地策略)[评分={}，RMJ成功={}/3年，累计RMJ剧本点={}，最终技能点={}，全局吃面={}碗] | B-A评分差={}",
            a.score,
            a.rmj_ok,
            a.scenario_pt,
            a.skill_pt,
            a.eat_count,
            b.score,
            b.rmj_ok,
            b.scenario_pt,
            b.skill_pt,
            b.eat_count,
            b.score - a.score
        );
        a_results.push(a);
        b_results.push(b);
    }

    print_summary("A(原策略)", &a_results);
    print_summary("B(本地策略)", &b_results);
    let mean_a = a_results.iter().map(|x| x.score as f64).sum::<f64>() / RUNS as f64;
    let mean_b = b_results.iter().map(|x| x.score as f64).sum::<f64>() / RUNS as f64;
    println!("DELTA B平均评分-A平均评分={:.0}（正数表示B本地策略更高）", mean_b - mean_a);
    Ok(())
}
