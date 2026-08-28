//! 手写 vs MCTS 对比工具：同 seed 跑两种策略，输出评分对比。
//!
//! 用法（环境变量控制）：
//! - `配卡索引`: 81 / 97 / 76
//! - `搜索次数`: MCTS search_n，默认 1024
//! - `每策略局数`: 默认 10

use std::{env, fs::File, io::Write};

use anyhow::{Context, Result, ensure};
use umasim::{
    bench::{self, CardPickOpts, DeckComposition},
    gamedata::{GAMECONSTANTS, init_global_with_config},
    global,
    search::SearchConfig,
    trainer::{LoggingTrainer, RecommendedRamenTrainer, RamenMctsTrainer},
    utils::{get_workspace_root, load_game_config}
};

const BASE_SEED: u64 = 995_100;
const UMA: u32 = 102601;
const FRIEND: u32 = 303054;
const INHERIT: umasim::game::InheritInfo = umasim::game::InheritInfo {
    blue_count: [15, 0, 0, 0, 3],
    extra_count: [10, 10, 20, 20, 20, 40]
};

fn composition(index: usize) -> Result<DeckComposition> {
    let mut all = Vec::new();
    for speed in 0..=3 {
        for stamina in 0..=3 {
            for power in 0..=3 {
                for guts in 0..=3 {
                    for wisdom in 0..=3 {
                        let counts = [speed, stamina, power, guts, wisdom];
                        if counts.iter().sum::<usize>() == 5 {
                            all.push(DeckComposition { counts, name: String::new() });
                        }
                    }
                }
            }
        }
    }
    all.get(index).cloned().with_context(|| format!("配卡索引越界: {index}"))
}

fn main() -> Result<()> {
    let composition_index: usize = env::var("配卡索引")?.parse()?;
    let search_n: usize = env::var("搜索次数").map_or(Ok(1024usize), |v| v.parse())?;
    let runs: u64 = env::var("每策略局数").map_or(Ok(10u64), |v| v.parse())?;

    std::env::set_current_dir(get_workspace_root()?)?;
    init_global_with_config(&load_game_config()?)?;
    let composition = composition(composition_index)?;
    let reps = bench::select_representatives(&CardPickOpts::default())?;
    let deck = composition.build_deck(&reps.picked, FRIEND)?;

    let mut file = File::create("手写vs蒙特卡洛对比.csv")?;
    writeln!(file, "配卡索引,配卡名,局序号,手写总分,手写技能点,手写属性分,MCTS总分,MCTS技能点,MCTS属性分,分差,MCTS胜")?;

    let mut hw_scores = Vec::new();
    let mut mcts_scores = Vec::new();

    for run_index in 0..runs {
        // 手写策略
        let hw_trainer = LoggingTrainer::new(RecommendedRamenTrainer::new(), run_index);
        let hw = bench::run_seeded(UMA, &deck, &INHERIT, BASE_SEED, run_index, &hw_trainer)?;

        // MCTS 策略
        let mcts_trainer = LoggingTrainer::new(
            RamenMctsTrainer::new(SearchConfig::default().with_search_n(search_n)),
            run_index
        );
        let mcts = bench::run_seeded(UMA, &deck, &INHERIT, BASE_SEED, run_index, &mcts_trainer)?;

        let diff = mcts.score - hw.score;
        let mcts_win = mcts.score > hw.score;

        writeln!(
            file,
            "{composition_index},{},{run_index},{},{},{},{},{},{},{diff},{}",
            composition.name(),
            hw.score, hw.skill_pt, status_score(&hw.five_status),
            mcts.score, mcts.skill_pt, status_score(&mcts.five_status),
            if mcts_win { "是" } else { "否" }
        )?;

        hw_scores.push(hw.score);
        mcts_scores.push(mcts.score);

        println!(
            "局{run_index}: 手写={} MCTS={} 差={:+} {}",
            hw.score, mcts.score, diff,
            if mcts_win { "→MCTS胜" } else if mcts.score < hw.score { "→手写胜" } else { "平" }
        );
    }

    let hw_avg = hw_scores.iter().sum::<i32>() as f64 / hw_scores.len() as f64;
    let mcts_avg = mcts_scores.iter().sum::<i32>() as f64 / mcts_scores.len() as f64;
    let hw_wins = hw_scores.iter().zip(mcts_scores.iter()).filter(|(h, m)| h > m).count();
    let mcts_wins = hw_scores.iter().zip(mcts_scores.iter()).filter(|(h, m)| m > h).count();

    println!("\n=== 汇总 ===");
    println!("手写均分: {hw_avg:.0}  (min={}, max={})", hw_scores.iter().min().unwrap(), hw_scores.iter().max().unwrap());
    println!("MCTS均分: {mcts_avg:.0}  (min={}, max={})", mcts_scores.iter().min().unwrap(), mcts_scores.iter().max().unwrap());
    println!("均分差: {:+.0}", mcts_avg - hw_avg);
    println!("胜/平/负: MCTS={mcts_wins} 平={} 手写={hw_wins}", runs as usize - mcts_wins - hw_wins);

    ensure!(runs > 0, "局数必须大于0");
    Ok(())
}

fn status_score(status: &[i32; 5]) -> i32 {
    let constants = global!(GAMECONSTANTS);
    status
        .iter()
        .map(|&value| constants.five_status_final_score[(value.max(0) as usize).min(constants.five_status_final_score.len() - 1)])
        .sum()
}
