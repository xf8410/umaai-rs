//! 第三年拉面地区组合全局评分矩阵。
//! 每个第三年组合都用生产配置跑完整育成，固定第1/2年策略与 MCTS 配置，
//! 只把第3年 RegionSelect 强制为当前组合；不使用静态 score_region。

use std::{env, path::Path};
use anyhow::{Result, bail};
use rand::prelude::StdRng;
use serde::Serialize;
use umasim::{
    bench,
    game::{Game, Trainer, InheritInfo, ramen::{Operation, RamenAction, RamenGame, rules::get_region_combinations}},
    gamedata::init_global_with_config,
    search::SearchConfig,
    trainer::{RamenMctsTrainer, RamenSearchStages},
    utils::{get_workspace_root, load_game_config},
};

const UMA: u32 = 101901;
const DECK: [u32; 6] = [303124, 303114, 303134, 303074, 303094, 303054];
const INHERIT: InheritInfo = InheritInfo { blue_count: [15, 0, 0, 0, 3], extra_count: [10, 10, 20, 20, 20, 40] };
const BASE_SEED: u64 = 2_026_082_500;

#[derive(Serialize)]
struct Row { combo_index: usize, region_ids: String, run: usize, score: i32, skill_pt: i32, scenario_pt: i32, rmj_success: usize, status: [i32; 5] }

struct FixedY3 { inner: RamenMctsTrainer, combo: [usize; 3] }
impl Trainer<RamenGame> for FixedY3 {
    fn select_action(&self, game: &RamenGame, actions: &[RamenAction], rng: &mut StdRng) -> Result<usize> {
        if game.turn() == 47 && actions.iter().all(|a| matches!(a.operation, Operation::RegionSelect(_))) {
            return actions.iter().position(|a| a.operation == Operation::RegionSelect(self.combo))
                .ok_or_else(|| anyhow::anyhow!("第三年组合 {:?} 不在候选集中", self.combo));
        }
        self.inner.select_action(game, actions, rng)
    }
    fn select_choice(&self, game: &RamenGame, choices: &[Vec<umasim::gamedata::EventChoice>], rng: &mut StdRng) -> Result<usize> { self.inner.select_choice(game, choices, rng) }
    fn select_event_choice(&self, game: &RamenGame, event: &umasim::gamedata::EventData, choices: &[Vec<umasim::gamedata::EventChoice>], rng: &mut StdRng) -> Result<usize> { self.inner.select_event_choice(game, event, choices, rng) }
    fn last_breakdown(&self) -> Option<String> { self.inner.last_breakdown() }
}

fn main() -> Result<()> {
    env::set_current_dir(get_workspace_root()?)?;
    let config = load_game_config()?;
    init_global_with_config(&config)?;
    let runs: usize = env::var("每组合局数").unwrap_or_else(|_| "10".into()).parse()?;
    let start: usize = env::var("起始组合").unwrap_or_else(|_| "0".into()).parse()?;
    let end: usize = env::var("结束组合").unwrap_or_else(|_| "120".into()).parse()?;
    let combos = get_region_combinations(2)?;
    if end > combos.len() || start >= end { bail!("组合范围无效: {start}..{end}"); }
    let mut w = csv::Writer::from_path(Path::new("第三年拉面组合评分.csv"))?;
    for (idx, combo) in combos[start..end].iter().enumerate() {
        let combo_index = start + idx;
        for run in 0..runs {
            let (mut rng, rule_master) = bench::seeded_rngs(BASE_SEED, (combo_index * runs + run) as u64);
            let mut game = RamenGame::newgame(UMA, &DECK, INHERIT)?;
            game.set_rule_master(rule_master);
            let search = SearchConfig::new_game_config(&config);
            let trainer = FixedY3 {
                inner: RamenMctsTrainer::new(search).with_stages(RamenSearchStages::parse("train,ramen")?),
                combo: *combo,
            };
            game.run_full_game(&trainer, &mut rng)?;
            w.serialize(Row {
                combo_index, region_ids: combo.iter().map(usize::to_string).collect::<Vec<_>>().join("/"), run,
                score: game.uma.calc_score(), skill_pt: game.uma.skill_pt, scenario_pt: game.ramen.scenario_pt,
                rmj_success: game.ramen.rmj_results.iter().filter(|&&x| x).count(), status: game.uma.five_status,
            })?;
        }
        w.flush()?;
        println!("组合 {combo_index:03} {:?} 完成 {runs} 局", combo);
    }
    Ok(())
}
