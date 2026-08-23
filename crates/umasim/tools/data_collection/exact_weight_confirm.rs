//! 初筛强候选的正式 preset 精确隔离复赛。

use std::{env, fs::File, io::Write};

use anyhow::{Context, Result, ensure};
use umasim::{
    bench::{self, CardPickOpts, DeckComposition},
    gamedata::{GAMECONSTANTS, init_global_with_config},
    global,
    trainer::{LoggingTrainer, RecommendedRamenTrainer},
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

fn status_score(status: &[i32; 5]) -> i32 {
    let constants = global!(GAMECONSTANTS);
    status.iter().map(|&value| {
        constants.five_status_final_score
            [(value.max(0) as usize).min(constants.five_status_final_score.len() - 1)]
    }).sum()
}

fn main() -> Result<()> {
    let variant = env::var("VARIANT")?;
    let composition_index: usize = env::var("COMPOSITION_INDEX")?.parse()?;
    let gap: f32 = env::var("GAP")?.parse::<f32>()? / 100.0;
    let overflow: f32 = env::var("OVERFLOW")?.parse::<f32>()? / 100.0;
    let pt: f32 = env::var("PT")?.parse()?;
    let shard: u64 = env::var("SHARD")?.parse()?;
    let runs: u64 = env::var("RUNS_PER_SHARD")?.parse()?;

    std::env::set_current_dir(get_workspace_root()?)?;
    init_global_with_config(&load_game_config()?)?;
    let composition = composition(composition_index)?;
    let reps = bench::select_representatives(&CardPickOpts::default())?;
    let deck = composition.build_deck(&reps.picked, FRIEND)?;
    let mut file = File::create("exact-weight-result.csv")?;
    writeln!(file, "variant,composition_index,composition,gap,overflow,pt,shard,run_index,base_score,candidate_score,base_pt,candidate_pt,base_status,candidate_status,identical")?;

    for offset in 0..runs {
        let run_index = shard * runs + offset;
        let base_trainer = LoggingTrainer::new(RecommendedRamenTrainer::new(), run_index);
        let candidate_trainer = LoggingTrainer::new(
            RecommendedRamenTrainer::with_weight_overrides([pt, pt, pt], gap, overflow),
            run_index
        );
        let base = bench::run_seeded(UMA, &deck, &INHERIT, BASE_SEED, run_index, &base_trainer)?;
        let candidate = bench::run_seeded(UMA, &deck, &INHERIT, BASE_SEED, run_index, &candidate_trainer)?;
        let identical = base.score == candidate.score
            && base.skill_pt == candidate.skill_pt
            && base.five_status == candidate.five_status;
        writeln!(file, "{variant},{composition_index},{},{gap},{overflow},{pt},{shard},{run_index},{},{},{},{},{},{},{identical}",
            composition.name(), base.score, candidate.score, base.skill_pt, candidate.skill_pt,
            status_score(&base.five_status), status_score(&candidate.five_status))?;
    }
    ensure!(runs > 0, "每分片局数必须大于0");
    Ok(())
}
