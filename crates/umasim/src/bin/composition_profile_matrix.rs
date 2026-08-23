//! 五种指定配卡的 v29-special / 配卡画像策略配对矩阵。

use std::{env, fs::File, io::Write};

use anyhow::Result;
use umasim::{
    bench::{self, CardPickOpts, DeckComposition},
    game::InheritInfo,
    gamedata::init_global_with_config,
    trainer::{CompositionRamenTrainer, LoggingTrainer, V29SpecialTrainer},
    utils::{get_workspace_root, load_game_config},
};

const UMA: u32 = 102601;
const FRIEND: u32 = 303054;
const BASE_SEED: u64 = 61444;
const INHERIT: InheritInfo = InheritInfo {
    blue_count: [15, 0, 0, 0, 3],
    extra_count: [10, 10, 20, 20, 20, 40],
};

fn builds() -> [DeckComposition; 5] {
    [
        DeckComposition {
            name: "3speed-1stamina-1wisdom".into(),
            counts: [3, 1, 0, 0, 1],
        },
        DeckComposition {
            name: "2speed-2stamina-1wisdom".into(),
            counts: [2, 2, 0, 0, 1],
        },
        DeckComposition {
            name: "2power-3wisdom".into(),
            counts: [0, 0, 2, 0, 3],
        },
        DeckComposition {
            name: "2speed-1stamina-2wisdom".into(),
            counts: [2, 1, 0, 0, 2],
        },
        DeckComposition {
            name: "1speed-1stamina-3wisdom".into(),
            counts: [1, 1, 0, 0, 3],
        },
    ]
}

fn main() -> Result<()> {
    std::env::set_current_dir(get_workspace_root()?)?;
    init_global_with_config(&load_game_config()?)?;
    let shard: u64 = env::var("SHARD").unwrap_or_else(|_| "0".into()).parse()?;
    let runs: u64 = env::var("RUNS_PER_SHARD").unwrap_or_else(|_| "100".into()).parse()?;
    let reps = bench::select_representatives(&CardPickOpts::default())?;
    let mut out = File::create("composition-profile-result.csv")?;
    writeln!(
        out,
        "build,deck,run_idx,baseline_score,profile_score,baseline_pt,profile_pt,baseline_rmj,profile_rmj"
    )?;
    for build in builds() {
        let deck = build.build_deck(&reps.picked, FRIEND)?;
        let deck_text = deck.iter().map(u32::to_string).collect::<Vec<_>>().join("/");
        for off in 0..runs {
            let run_idx = shard * runs + off;
            let baseline = bench::run_seeded(
                UMA,
                &deck,
                &INHERIT,
                BASE_SEED,
                run_idx,
                &LoggingTrainer::new(V29SpecialTrainer::new()?, run_idx),
            )?;
            let profile = bench::run_seeded(
                UMA,
                &deck,
                &INHERIT,
                BASE_SEED,
                run_idx,
                &LoggingTrainer::new(CompositionRamenTrainer::new()?, run_idx),
            )?;
            writeln!(
                out,
                "{},{},{},{},{},{},{},{},{}",
                build.name(),
                deck_text,
                run_idx,
                baseline.score,
                profile.score,
                baseline.skill_pt,
                profile.skill_pt,
                baseline.rmj_ok,
                profile.rmj_ok
            )?;
        }
    }
    Ok(())
}
