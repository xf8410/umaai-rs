//! 3速1耐1智与2速2耐1智 × 第三年120个地区组合 × 每组合100个相同seed。
use std::{env, path::Path};
use anyhow::{Context, Result, ensure};
use rand::prelude::StdRng;
use serde::Serialize;
use umasim::{
    bench::{self, CardPickOpts, DeckComposition},
    game::{Game, InheritInfo, Trainer, ramen::{Operation, RamenAction, RamenGame, rules::get_region_combinations}},
    gamedata::{EventChoice, EventData, init_global_with_config},
    trainer::RecommendedRamenTrainer,
    utils::{get_workspace_root, load_game_config},
};

const UMA: u32 = 102601;
const FRIEND: u32 = 303054;
const BASE_SEED: u64 = 884_400;
const INHERIT: InheritInfo = InheritInfo {
    blue_count: [15, 0, 0, 0, 3],
    extra_count: [10, 10, 20, 20, 20, 40],
};

#[derive(Serialize)]
struct Row {
    build: String,
    composition: String,
    deck: String,
    combo_index: usize,
    region_ids: String,
    run: usize,
    score: i32,
    skill_pt: i32,
    speed: i32,
    stamina: i32,
    power: i32,
    guts: i32,
    wisdom: i32,
}

struct FixedY3 {
    inner: RecommendedRamenTrainer,
    combo: [usize; 3],
}

impl Trainer<RamenGame> for FixedY3 {
    fn select_action(&self, game: &RamenGame, actions: &[RamenAction], rng: &mut StdRng) -> Result<usize> {
        if game.turn() == 47 && actions.iter().all(|a| matches!(a.operation, Operation::RegionSelect(_))) {
            return actions
                .iter()
                .position(|a| a.operation == Operation::RegionSelect(self.combo))
                .with_context(|| format!("第三年地区组合不在候选中: {:?}", self.combo));
        }
        self.inner.select_action(game, actions, rng)
    }

    fn select_choice(&self, game: &RamenGame, choices: &[Vec<EventChoice>], rng: &mut StdRng) -> Result<usize> {
        self.inner.select_choice(game, choices, rng)
    }

    fn select_event_choice(&self, game: &RamenGame, event: &EventData, choices: &[Vec<EventChoice>], rng: &mut StdRng) -> Result<usize> {
        self.inner.select_event_choice(game, event, choices, rng)
    }

    fn last_breakdown(&self) -> Option<String> {
        self.inner.last_breakdown()
    }
}

fn main() -> Result<()> {
    env::set_current_dir(get_workspace_root()?)?;
    init_global_with_config(&load_game_config()?)?;
    let start: usize = env::var("COMBO_START").context("缺少 COMBO_START")?.parse()?;
    let end: usize = env::var("COMBO_END").context("缺少 COMBO_END")?.parse()?;
    let runs: usize = env::var("RUNS").unwrap_or_else(|_| "100".into()).parse()?;
    ensure!(runs == 100, "正式矩阵要求每组合100局，实际{runs}");

    let combos = get_region_combinations(2)?;
    ensure!(combos.len() == 120, "第三年地区组合应为120，实际{}", combos.len());
    ensure!(start < end && end <= 120, "地区分片无效: {start}..{end}");

    let reps = bench::select_representatives(&CardPickOpts::default())?;
    let builds = [("3速1耐1智", [3, 1, 0, 0, 1]), ("2速2耐1智", [2, 2, 0, 0, 1])];
    let mut writer = csv::Writer::from_path(Path::new("requested-decks-region-matrix.csv"))?;

    for (build, counts) in builds {
        let composition = DeckComposition { counts, name: String::new() };
        let deck = composition.build_deck(&reps.picked, FRIEND)?;
        let deck_text = deck.iter().map(u32::to_string).collect::<Vec<_>>().join("/");
        for (offset, combo) in combos[start..end].iter().enumerate() {
            let combo_index = start + offset;
            let region_ids = combo.iter().map(usize::to_string).collect::<Vec<_>>().join("/");
            for run in 0..runs {
                // 所有地区组合和两套配卡共用同一组100个seed，便于严格配对。
                let (mut rng, rule_master) = bench::seeded_rngs(BASE_SEED, run as u64);
                let mut game = RamenGame::newgame(UMA, &deck, INHERIT)?;
                game.set_rule_master(rule_master);
                let trainer = FixedY3 { inner: RecommendedRamenTrainer::new(), combo: *combo };
                game.run_full_game(&trainer, &mut rng)?;
                let status = game.uma.five_status;
                writer.serialize(Row {
                    build: build.into(),
                    composition: composition.name(),
                    deck: deck_text.clone(),
                    combo_index,
                    region_ids: region_ids.clone(),
                    run,
                    score: game.uma.calc_score(),
                    skill_pt: game.uma.skill_pt,
                    speed: status[0], stamina: status[1], power: status[2], guts: status[3], wisdom: status[4],
                })?;
            }
        }
    }
    writer.flush()?;
    Ok(())
}
