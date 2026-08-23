//! 第三年窄邻域：固定前两年，仅扫描第三年吃面对盘和轻微降低 PT 权重。

use std::{env, path::Path};

use anyhow::Result;
use rand::prelude::StdRng;
use umasim::{
    bench::{self, CardPickOpts, DeckComposition},
    game::{Game, InheritInfo, Trainer, ramen::{RamenAction, RamenGame}},
    gamedata::{EventChoice, EventData, init_global_with_config},
    trainer::RecommendedRamenTrainer,
    utils::{get_workspace_root, load_game_config}
};

const DEFAULT_BASE_SEED: u64 = 995_100;
const UMA: u32 = 102601;
const FRIEND: u32 = 303054;
const INHERIT: InheritInfo = InheritInfo {
    blue_count: [15, 0, 0, 0, 3],
    extra_count: [10, 10, 20, 20, 20, 40]
};

struct PhaseTrainer {
    years: [RecommendedRamenTrainer; 3]
}

impl PhaseTrainer {
    fn year(game: &RamenGame) -> usize {
        if game.turn() < 24 { 0 } else if game.turn() < 48 { 1 } else { 2 }
    }

    fn candidate(name: &str) -> Result<Self> {
        let (y3_pt, y3_window) = match name {
            "对盘08" => (32.0, 0.08),
            "对盘10" => (32.0, 0.10),
            "对盘11" => (32.0, 0.11),
            "对盘12" => (32.0, 0.12),
            "对盘13" => (32.0, 0.13),
            "对盘14" => (32.0, 0.14),
            "对盘15" => (32.0, 0.15),
            "PT28对盘12" => (28.0, 0.12),
            "PT30对盘12" => (30.0, 0.12),
            _ => anyhow::bail!("未知候选: {name}")
        };
        let make = |pt: f32, window: f32| {
            RecommendedRamenTrainer::with_experiment_overrides(
                [pt; 3], 0.75, 1.0, 220.0, window, 20.0, 12.0, 8.0
            )
        };
        Ok(Self {
            // 前两年冻结为 PT32 / 对盘15；第三年才使用候选值。
            years: [make(32.0, 0.15), make(32.0, 0.15), make(y3_pt, y3_window)]
        })
    }
}

impl Trainer<RamenGame> for PhaseTrainer {
    fn select_action(&self, game: &RamenGame, actions: &[RamenAction], rng: &mut StdRng) -> Result<usize> {
        self.years[Self::year(game)].select_action(game, actions, rng)
    }
    fn select_choice(&self, game: &RamenGame, choices: &[Vec<EventChoice>], rng: &mut StdRng) -> Result<usize> {
        self.years[Self::year(game)].select_choice(game, choices, rng)
    }
    fn select_event_choice(
        &self, game: &RamenGame, event: &EventData, choices: &[Vec<EventChoice>], rng: &mut StdRng
    ) -> Result<usize> {
        self.years[Self::year(game)].select_event_choice(game, event, choices, rng)
    }
}

fn deck() -> Result<[u32; 6]> {
    let composition = DeckComposition { counts: [3, 1, 0, 0, 1], name: String::new() };
    let representatives = bench::select_representatives(&CardPickOpts::default())?;
    composition.build_deck(&representatives.picked, FRIEND)
}

fn main() -> Result<()> {
    std::env::set_current_dir(get_workspace_root()?)?;
    init_global_with_config(&load_game_config()?)?;
    let name = env::var("候选方案")?;
    let base_seed: u64 = env::var("基础种子")
        .map_or(Ok(DEFAULT_BASE_SEED), |value| value.parse())?;
    let shard: u64 = env::var("分片序号")?.parse()?;
    let runs: u64 = env::var("每分片局数")?.parse()?;
    let deck = deck()?;
    let mut rows = Vec::with_capacity(runs as usize);
    for offset in 0..runs {
        let run_index = shard * runs + offset;
        let (mut rng, rule_master) = bench::seeded_rngs(base_seed, run_index);
        let mut game = RamenGame::newgame(UMA, &deck, INHERIT.clone())?;
        game.set_rule_master(rule_master);
        game.run_full_game(&PhaseTrainer::candidate(&name)?, &mut rng)?;
        rows.push(vec![
            name.clone(), base_seed.to_string(), run_index.to_string(), game.uma.calc_score().to_string(),
            game.uma.skill_pt.to_string(), game.ramen.scenario_pt.to_string(),
            game.ramen.rmj_results.iter().filter(|&&ok| ok).count().to_string()
        ]);
    }
    bench::write_csv(
        Path::new("第三年窄邻域.csv"),
        &["方案", "基础种子", "局序号", "总分", "技能点", "最终拉面点", "RMJ成功年数"],
        &rows
    )
}
