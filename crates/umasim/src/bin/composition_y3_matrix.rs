//! 101种速/耐/力/根/智数量构成 × 第三年120种地区组合矩阵。
use std::{env, path::Path};
use anyhow::{Context, Result, bail, ensure};
use rand::prelude::StdRng;
use serde::Serialize;
use umasim::{bench::{self, CardPickOpts, DeckComposition}, game::{Game, InheritInfo, Trainer, ramen::{Operation, RamenAction, RamenGame, rules::get_region_combinations}}, gamedata::{EventChoice, EventData, init_global_with_config}, search::SearchConfig, trainer::{RamenMctsTrainer, RamenSearchStages}, utils::{get_workspace_root, load_game_config}};

const UMA: u32 = 102601;
const FRIEND: u32 = 303054;
const INHERIT: InheritInfo = InheritInfo { blue_count: [12, 0, 0, 0, 6], extra_count: [10, 0, 0, 20, 20, 40] };
const BASE_SEED: u64 = 2_026_082_500;

#[derive(Serialize)]
struct Row { composition_index: usize, composition: String, deck: String, combo_index: usize, region_ids: String, run: usize, score: i32, skill_pt: i32, scenario_pt: i32, rmj_success: usize, speed: i32, stamina: i32, power: i32, guts: i32, wisdom: i32 }
struct FixedY3 { inner: RamenMctsTrainer, combo: [usize; 3] }
impl Trainer<RamenGame> for FixedY3 {
    fn select_action(&self, game: &RamenGame, actions: &[RamenAction], rng: &mut StdRng) -> Result<usize> {
        if game.turn() == 47 && actions.iter().all(|a| matches!(a.operation, Operation::RegionSelect(_))) {
            return actions.iter().position(|a| a.operation == Operation::RegionSelect(self.combo)).ok_or_else(|| anyhow::anyhow!("第三年组合不在候选集中: {:?}", self.combo));
        }
        self.inner.select_action(game, actions, rng)
    }
    fn select_choice(&self, game: &RamenGame, choices: &[Vec<EventChoice>], rng: &mut StdRng) -> Result<usize> { self.inner.select_choice(game, choices, rng) }
    fn select_event_choice(&self, game: &RamenGame, event: &EventData, choices: &[Vec<EventChoice>], rng: &mut StdRng) -> Result<usize> { self.inner.select_event_choice(game, event, choices, rng) }
    fn last_breakdown(&self) -> Option<String> { self.inner.last_breakdown() }
}
fn compositions() -> Vec<DeckComposition> {
    let mut out = Vec::new();
    for a in 0..=3 { for b in 0..=3 { for c in 0..=3 { for d in 0..=3 { for e in 0..=3 {
        let counts = [a,b,c,d,e]; if counts.iter().sum::<usize>() == 5 { out.push(DeckComposition { counts, name: String::new() }); }
    }}}}}
    out
}
fn main() -> Result<()> {
    env::set_current_dir(get_workspace_root()?)?;
    let config = load_game_config()?; init_global_with_config(&config)?;
    let ci: usize = env::var("COMPOSITION_INDEX").context("缺少 COMPOSITION_INDEX")?.parse()?;
    let runs: usize = env::var("RUNS").unwrap_or_else(|_| "10".into()).parse()?;
    let cs = compositions(); ensure!(cs.len() == 101, "配卡构成数量不是101: {}", cs.len());
    let comp = cs.get(ci).with_context(|| format!("配卡构成索引越界: {ci}"))?;
    let reps = bench::select_representatives(&CardPickOpts::default())?;
    let deck = comp.build_deck(&reps.picked, FRIEND)?;
    let combos = get_region_combinations(2)?; ensure!(combos.len() == 120, "地区组合数量不是120: {}", combos.len());
    let mut writer = csv::Writer::from_path(Path::new("composition-y3-matrix.csv"))?;
    let deck_text = deck.iter().map(u32::to_string).collect::<Vec<_>>().join("/");
    for (combo_index, combo) in combos.iter().enumerate() { for run in 0..runs {
        let (mut rng, rule_master) = bench::seeded_rngs(BASE_SEED, (ci * combos.len() * runs + combo_index * runs + run) as u64);
        let mut game = RamenGame::newgame(UMA, &deck, INHERIT)?; game.set_rule_master(rule_master);
        let trainer = FixedY3 { inner: RamenMctsTrainer::new(SearchConfig::new_game_config(&config)).with_stages(RamenSearchStages::parse("train,ramen")?), combo: *combo };
        game.run_full_game(&trainer, &mut rng)?; let s = game.uma.five_status;
        writer.serialize(Row { composition_index: ci, composition: comp.name(), deck: deck_text.clone(), combo_index, region_ids: combo.iter().map(usize::to_string).collect::<Vec<_>>().join("/"), run, score: game.uma.calc_score(), skill_pt: game.uma.skill_pt, scenario_pt: game.ramen.scenario_pt, rmj_success: game.ramen.rmj_results.iter().filter(|&&x| x).count(), speed:s[0], stamina:s[1], power:s[2], guts:s[3], wisdom:s[4] })?;
    } println!("配卡 {ci:03} 组合 {combo_index:03} 完成 {runs} 局"); }
    writer.flush()?; Ok(())
}
