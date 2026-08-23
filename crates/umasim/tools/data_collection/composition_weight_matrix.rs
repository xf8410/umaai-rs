//! 全部合法配卡构成 × 属性缺口权重 × 溢出抑制权重 × 技能点权重矩阵。

use std::{env, fs::File, io::Write, sync::Mutex};

use anyhow::{Context, Result, ensure};
use rand::prelude::StdRng;
use umasim::{
    bench::{self, CardPickOpts, DeckComposition},
    game::{
        Game,
        InheritInfo,
        Trainer,
        ramen::{RamenAction, RamenGame}
    },
    gamedata::{EventChoice, EventData, GAMECONSTANTS, init_global_with_config},
    global,
    trainer::{LocalRamenTrainer, LoggingTrainer, RecommendedRamenTrainer},
    utils::{get_workspace_root, load_game_config}
};

const BASE_SEED: u64 = 884_400;
const UMA: u32 = 102601;
const FRIEND: u32 = 303054;
const INHERIT: InheritInfo = InheritInfo {
    blue_count: [15, 0, 0, 0, 3],
    extra_count: [10, 10, 20, 20, 20, 40]
};
const WEIGHTS: [u32; 5] = [0, 25, 50, 75, 100];
const PT_WEIGHTS: [u32; 5] = [32, 48, 64, 80, 96];

struct CandidateTrainer {
    years: [LocalRamenTrainer; 3],
    last_year: Mutex<Option<usize>>
}

impl CandidateTrainer {
    fn new(pt: u32, gap: u32, overflow: u32) -> Result<Self> {
        let make = |vital_rest: u32| {
            LocalRamenTrainer::matrix_variant(&format!(
                "pt{pt}-sac140-long-fail0-structall-rpt200-window10-look0-samples1-rawfail-cook240-eatguard-friendrest-friendcap025-friendspecial4-specialdynamic-vrest{vital_rest}-statusdyn-gap{gap}-over{overflow}"
            ))
        };
        Ok(Self {
            years: [make(30)?, make(30)?, make(0)?],
            last_year: Mutex::new(None)
        })
    }

    fn year(game: &RamenGame) -> usize {
        if game.turn() < 24 {
            0
        } else if game.turn() < 48 {
            1
        } else {
            2
        }
    }
}

impl Trainer<RamenGame> for CandidateTrainer {
    fn select_action(&self, game: &RamenGame, actions: &[RamenAction], rng: &mut StdRng) -> Result<usize> {
        let year = Self::year(game);
        *self.last_year.lock().unwrap() = Some(year);
        self.years[year].select_action(game, actions, rng)
    }

    fn select_choice(&self, game: &RamenGame, choices: &[Vec<EventChoice>], rng: &mut StdRng) -> Result<usize> {
        let year = Self::year(game);
        *self.last_year.lock().unwrap() = Some(year);
        self.years[year].select_choice(game, choices, rng)
    }

    fn select_event_choice(
        &self, game: &RamenGame, event: &EventData, choices: &[Vec<EventChoice>], rng: &mut StdRng
    ) -> Result<usize> {
        let year = Self::year(game);
        *self.last_year.lock().unwrap() = Some(year);
        self.years[year].select_event_choice(game, event, choices, rng)
    }

    fn last_breakdown(&self) -> Option<String> {
        let year = (*self.last_year.lock().ok()?)?;
        self.years[year].last_breakdown()
    }
}

fn enumerate_compositions() -> Vec<DeckComposition> {
    let mut result = Vec::new();
    for speed in 0..=3 {
        for stamina in 0..=3 {
            for power in 0..=3 {
                for guts in 0..=3 {
                    for wisdom in 0..=3 {
                        let counts = [speed, stamina, power, guts, wisdom];
                        if counts.iter().sum::<usize>() == 5 {
                            result.push(DeckComposition { counts, name: String::new() });
                        }
                    }
                }
            }
        }
    }
    result
}

fn status_score(status: &[i32; 5]) -> i32 {
    let constants = global!(GAMECONSTANTS);
    status
        .iter()
        .map(|&value| {
            constants.five_status_final_score
                [(value.max(0) as usize).min(constants.five_status_final_score.len() - 1)]
        })
        .sum()
}

fn main() -> Result<()> {
    let composition_index: usize = env::var("COMPOSITION_INDEX")
        .context("缺少 COMPOSITION_INDEX")?
        .parse()?;
    let runs: u64 = env::var("RUNS").unwrap_or_else(|_| "300".into()).parse()?;
    ensure!(runs == 300, "正式矩阵要求每个方案严格运行300局，实际为{runs}");

    std::env::set_current_dir(get_workspace_root()?)?;
    init_global_with_config(&load_game_config()?)?;

    let compositions = enumerate_compositions();
    ensure!(compositions.len() == 101, "合法配卡构成数量应为101，实际为{}", compositions.len());
    let composition = compositions
        .get(composition_index)
        .with_context(|| format!("配卡构成索引越界：{composition_index}"))?;
    let representatives = bench::select_representatives(&CardPickOpts::default())?;
    let deck = composition.build_deck(&representatives.picked, FRIEND)?;
    let composition_name = composition.name();

    let mut baseline = Vec::with_capacity(runs as usize);
    for run_index in 0..runs {
        let trainer = LoggingTrainer::new(RecommendedRamenTrainer::new(), run_index);
        baseline.push(bench::run_seeded(UMA, &deck, &INHERIT, BASE_SEED, run_index, &trainer)?);
    }

    let mut file = File::create("composition-weight-result.csv")?;
    writeln!(
        file,
        "composition_index,composition,deck,gap_weight,overflow_weight,pt_weight,run_index,base_score,candidate_score,base_skill_pt,candidate_skill_pt,base_status_score,candidate_status_score,base_status_sum,candidate_status_sum"
    )?;
    let deck_text = deck.iter().map(u32::to_string).collect::<Vec<_>>().join("/");

    for gap in WEIGHTS {
        for overflow in WEIGHTS {
            for pt in PT_WEIGHTS {
                for run_index in 0..runs {
                    let trainer = LoggingTrainer::new(CandidateTrainer::new(pt, gap, overflow)?, run_index);
                    let candidate = bench::run_seeded(UMA, &deck, &INHERIT, BASE_SEED, run_index, &trainer)?;
                    let base = &baseline[run_index as usize];
                    writeln!(
                        file,
                        "{composition_index},{composition_name},{deck_text},{gap},{overflow},{pt},{run_index},{},{},{},{},{},{},{},{}",
                        base.score,
                        candidate.score,
                        base.skill_pt,
                        candidate.skill_pt,
                        status_score(&base.five_status),
                        status_score(&candidate.five_status),
                        base.five_status.iter().sum::<i32>(),
                        candidate.five_status.iter().sum::<i32>()
                    )?;
                }
            }
        }
    }
    Ok(())
}
