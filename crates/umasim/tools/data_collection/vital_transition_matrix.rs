//! P0 第3步：第三年吃面前后体力转移矩阵。

use std::{env, path::Path};
use anyhow::Result;
use umasim::{
    bench::{self, CardPickOpts, DeckComposition},
    game::{Game, InheritInfo, ramen::RamenGame},
    gamedata::init_global_with_config,
    trainer::RecommendedRamenTrainer,
    utils::{get_workspace_root, load_game_config},
};

const UMA: u32 = 102601;
const FRIEND: u32 = 303054;
const INHERIT: InheritInfo = InheritInfo {
    blue_count: [15, 0, 0, 0, 3],
    extra_count: [10, 10, 20, 20, 20, 40],
};

fn candidate(name: &str) -> Result<RecommendedRamenTrainer> {
    let args = match name {
        "无转移预算" => (0, 0, 0.0, 0, true),
        "训练后10轻罚" => (0, 10, 2.0, 0, true),
        "训练后15轻罚" => (0, 15, 2.0, 0, true),
        "训练后20轻罚" => (0, 20, 2.0, 0, true),
        "训练后15中罚" => (0, 15, 4.0, 0, true),
        "前20后10轻罚" => (20, 10, 2.0, 0, true),
        "前30后15轻罚" => (30, 15, 2.0, 0, true),
        "训练后硬底线0" => (0, 0, 0.0, 1, true),
        "无恢复视野后15" => (0, 15, 2.0, 0, false),
        _ => anyhow::bail!("未知候选: {name}"),
    };
    Ok(RecommendedRamenTrainer::with_vital_transition_overrides(
        args.0, args.1, args.2, args.3, args.4,
    ))
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
    let seed: u64 = env::var("基础种子")?.parse()?;
    let shard: u64 = env::var("分片序号")?.parse()?;
    let runs: u64 = env::var("每分片局数")?.parse()?;
    let deck = deck()?;
    let mut rows = Vec::with_capacity(runs as usize);
    for offset in 0..runs {
        let run_index = shard * runs + offset;
        let (mut rng, rule_master) = bench::seeded_rngs(seed, run_index);
        let mut game = RamenGame::newgame(UMA, &deck, INHERIT.clone())?;
        game.set_rule_master(rule_master);
        game.run_full_game(&candidate(&name)?, &mut rng)?;
        rows.push(vec![
            name.clone(), seed.to_string(), run_index.to_string(),
            game.uma.calc_score().to_string(), game.uma.skill_pt.to_string(),
            game.ramen.scenario_pt.to_string(),
            game.ramen.rmj_results.iter().filter(|&&ok| ok).count().to_string(),
        ]);
    }
    bench::write_csv(
        Path::new("吃后体力转移矩阵.csv"),
        &["方案", "基础种子", "局序号", "总分", "技能点", "最终拉面点", "RMJ成功年数"],
        &rows,
    )
}
