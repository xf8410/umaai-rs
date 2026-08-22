//! 最小策略 A/B：同一马娘、卡组、继承和 10 个 seed 比较上游与本地扩展策略。

use anyhow::Result;
use umasim::{
    bench::{self, GameOutcome},
    game::InheritInfo,
    gamedata::init_global_with_config,
    trainer::{LocalRamenTrainer, RamenHandwrittenTrainer},
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

fn print_summary(name: &str, outcomes: &[GameOutcome]) {
    let scores = outcomes.iter().map(|x| x.score as f64).collect::<Vec<_>>();
    let stats = bench::summarize(&scores);
    let rmj = outcomes.iter().map(|x| x.rmj_ok as f64).sum::<f64>() / outcomes.len() as f64;
    let pt = outcomes.iter().map(|x| x.scenario_pt as f64).sum::<f64>() / outcomes.len() as f64;
    let eat = outcomes.iter().map(|x| x.eat_count as f64).sum::<f64>() / outcomes.len() as f64;
    println!(
        "RESULT {name}: n={} mean={:.0} median={:.0} min={:.0} max={:.0} std={:.0} RMJ={:.2}/3 PT={:.0} eat={:.1}",
        outcomes.len(), stats.mean, stats.median, stats.min, stats.max, stats.std, rmj, pt, eat
    );
}

fn main() -> Result<()> {
    let root = get_workspace_root()?;
    std::env::set_current_dir(root)?;
    init_global_with_config(&load_game_config()?)?;

    let upstream = RamenHandwrittenTrainer::new();
    let local = LocalRamenTrainer::new();
    let mut upstream_results = Vec::with_capacity(RUNS);
    let mut local_results = Vec::with_capacity(RUNS);

    println!("Minimal A/B: {RUNS} games/strategy, seeds {BASE_SEED}..{}", BASE_SEED + RUNS as u64 - 1);
    for i in 0..RUNS {
        let seed = BASE_SEED + i as u64;
        let a = bench::run_seeded(UMA, &DECK, &INHERIT, seed, &upstream)?;
        let b = bench::run_seeded(UMA, &DECK, &INHERIT, seed, &local)?;
        println!(
            "seed={seed} upstream={} local={} delta={} RMJ={}/{}",
            a.score, b.score, b.score - a.score, a.rmj_ok, b.rmj_ok
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
