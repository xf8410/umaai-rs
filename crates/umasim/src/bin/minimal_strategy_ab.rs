//! 1000 组配对测试：上游手写基准策略与优化后的本地手写修正策略。

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

fn apply_cumulative_ramen_metrics(outcome: &mut GameOutcome, rows: &[DecisionLogRow]) -> Result<()> {
    let mut yearly_eat = [0_i32; 3];
    let mut total_pt = 0_i32;
    for row in rows {
        if row.stage != "RamenSelect" || !row.action_desc.starts_with("吃面/") {
            continue;
        }
        let year_idx = if row.turn < 24 { 0 } else if row.turn < 48 { 1 } else { 2 };
        total_pt += calc_ramen_pt_gain(year_idx, yearly_eat[year_idx])?;
        yearly_eat[year_idx] += 1;
    }
    let total_eat = yearly_eat.iter().sum();
    ensure!(total_eat > 0, "seed={} 未记录到任何吃面决策", outcome.seed);
    ensure!(total_pt > 0, "seed={} 累计 RMJ 剧本点异常为 0", outcome.seed);
    outcome.scenario_pt = total_pt;
    outcome.eat_count = total_eat;
    Ok(())
}

fn score_comparison(a: i32, b: i32) -> String {
    match a.cmp(&b) {
        std::cmp::Ordering::Less => format!("A比B少{}分", b - a),
        std::cmp::Ordering::Greater => format!("B比A少{}分", a - b),
        std::cmp::Ordering::Equal => "A与B同分".to_string()
    }
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

    println!("A=上游手写基准策略（RamenPolicy默认配置）。");
    println!("B=同一上游手写基准策略 + 优化后的本地长期收益修正；不是另外两套隐藏策略。");
    println!("RMJ剧本点用于年度RMJ判定/剧本加成；技能点用于购买技能；二者不是同一种点数。");

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
        let comparison = score_comparison(a.score, b.score);

        println!(
            "seed={seed} | A(上游手写基准)[评分={}，RMJ成功={}/3年，累计RMJ剧本点={}，最终技能点={}，全局吃面={}碗] | B(基准+优化本地修正)[评分={}，RMJ成功={}/3年，累计RMJ剧本点={}，最终技能点={}，全局吃面={}碗] | {comparison}",
            a.score, a.rmj_ok, a.scenario_pt, a.skill_pt, a.eat_count,
            b.score, b.rmj_ok, b.scenario_pt, b.skill_pt, b.eat_count
        );
        a_results.push(a);
        b_results.push(b);
    }

    print_summary("A(上游手写基准)", &a_results);
    print_summary("B(基准+优化本地修正)", &b_results);
    let mean_a = a_results.iter().map(|x| x.score as f64).sum::<f64>() / RUNS as f64;
    let mean_b = b_results.iter().map(|x| x.score as f64).sum::<f64>() / RUNS as f64;
    let rounded_a = mean_a.round() as i32;
    let rounded_b = mean_b.round() as i32;
    println!("DELTA 平均评分比较：{}", score_comparison(rounded_a, rounded_b));
    Ok(())
}
