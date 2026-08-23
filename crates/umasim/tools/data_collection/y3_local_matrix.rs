//! 根据 3速1耐1智结构诊断，只改变第三年的 PT、吃面对盘和 Hint 价值。

use std::{env, path::Path, sync::Mutex};

use anyhow::Result;
use rand::prelude::StdRng;
use umasim::{
    bench::{self, CardPickOpts, DeckComposition},
    game::{
        Game, InheritInfo, Trainer,
        ramen::{RamenAction, RamenGame}
    },
    gamedata::{EventChoice, EventData, init_global_with_config},
    trainer::RecommendedRamenTrainer,
    utils::{get_workspace_root, load_game_config}
};

const BASE_SEED: u64 = 995_100;
const UMA: u32 = 102601;
const FRIEND: u32 = 303054;
const INHERIT: InheritInfo = InheritInfo {
    blue_count: [15, 0, 0, 0, 3],
    extra_count: [10, 10, 20, 20, 20, 40]
};

struct PhaseTrainer {
    years: [RecommendedRamenTrainer; 3],
    last_year: Mutex<usize>
}

impl PhaseTrainer {
    fn year(game: &RamenGame) -> usize {
        if game.turn() < 24 { 0 } else if game.turn() < 48 { 1 } else { 2 }
    }

    fn candidate(name: &str) -> Result<Self> {
        // 前两年固定使用均值候选；第三年才应用矩阵变量。
        let base = (32.0, 0.15, 20.0, 12.0, 8.0);
        let y3 = match name {
            "均值基准" => base,
            "稳健基准" => (32.0, 0.12, 40.0, 10.0, 7.0),
            "第三年PT36" => (36.0, 0.15, 20.0, 12.0, 8.0),
            "第三年PT40" => (40.0, 0.15, 20.0, 12.0, 8.0),
            "第三年PT48" => (48.0, 0.15, 20.0, 12.0, 8.0),
            "第三年对盘12" => (32.0, 0.12, 20.0, 12.0, 8.0),
            "第三年对盘18" => (32.0, 0.18, 20.0, 12.0, 8.0),
            "第三年Hint10" => (32.0, 0.15, 20.0, 12.0, 10.0),
            "第三年组合" => (40.0, 0.18, 20.0, 12.0, 10.0),
            _ => anyhow::bail!("未知候选: {name}")
        };
        let make = |p: (f32, f32, f32, f32, f32)| {
            RecommendedRamenTrainer::with_experiment_overrides(
                [p.0; 3], 0.75, 1.0, 220.0, p.1, p.2, p.3, p.4
            )
        };
        Ok(Self {
            years: [make(base), make(base), make(y3)],
            last_year: Mutex::new(0)
        })
    }
}

impl Trainer<RamenGame> for PhaseTrainer {
    fn select_action(&self, game: &RamenGame, actions: &[RamenAction], rng: &mut StdRng) -> Result<usize> {
        let year = Self::year(game);
        if let Ok(mut slot) = self.last_year.lock() { *slot = year; }
        self.years[year].select_action(game, actions, rng)
    }

    fn select_choice(&self, game: &RamenGame, choices: &[Vec<EventChoice>], rng: &mut StdRng) -> Result<usize> {
        let year = Self::year(game);
        self.years[year].select_choice(game, choices, rng)
    }

    fn select_event_choice(
        &self, game: &RamenGame, event: &EventData, choices: &[Vec<EventChoice>], rng: &mut StdRng
    ) -> Result<usize> {
        let year = Self::year(game);
        self.years[year].select_event_choice(game, event, choices, rng)
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
    let shard: u64 = env::var("分片序号")?.parse()?;
    let runs: u64 = env::var("每分片局数")?.parse()?;
    let deck = deck()?;
    let mut rows = Vec::with_capacity(runs as usize);

    for offset in 0..runs {
        let run_index = shard * runs + offset;
        let (mut rng, rule_master) = bench::seeded_rngs(BASE_SEED, run_index);
        let mut game = RamenGame::newgame(UMA, &deck, INHERIT.clone())?;
        game.set_rule_master(rule_master);
        let trainer = PhaseTrainer::candidate(&name)?;
        game.run_full_game(&trainer, &mut rng)?;
        rows.push(vec![
            name.clone(), run_index.to_string(), game.uma.calc_score().to_string(),
            game.uma.skill_pt.to_string(), game.ramen.scenario_pt.to_string(),
            game.ramen.rmj_results.iter().filter(|&&ok| ok).count().to_string(),
            game.uma.five_status[0].to_string(), game.uma.five_status[1].to_string(),
            game.uma.five_status[2].to_string(), game.uma.five_status[3].to_string(),
            game.uma.five_status[4].to_string()
        ]);
    }
    bench::write_csv(
        Path::new("第三年局部矩阵.csv"),
        &["方案", "局序号", "总分", "技能点", "最终拉面点", "RMJ成功年数", "速度", "耐力", "力量", "根性", "智力"],
        &rows
    )
}
