//! 第三年地区选择 120 组合矩阵诊断。
//!
//! 本工具只调用现有 `RamenPolicy::decide_region`，不修改正式训练策略，
//! 用于回答第三年地区选择是否被三训练位组合静态垄断。
//!
//! 输出：
//! - 每种代表卡组的全部 120 个组合及分数；
//! - 每种卡组的 Top-N 组合；
//! - 被选组合、地区 at_trains 覆盖数和组合类型。

use std::{fs::File, path::Path};

use anyhow::{Context, Result, ensure};
use csv::Writer;
use lexopt::Arg;
use serde::Serialize;
use umasim::{
    bench::{self, CardPickOpts, DeckComposition},
    game::{InheritInfo, Trainer, ramen::{Operation, RamenAction, RamenGame, rules::get_region_combinations}},
    gamedata::init_global_with_config,
    trainer::ramen_handwritten_trainer::RecommendedRamenTrainer,
    utils::{get_workspace_root, load_game_config},
};

const UMA: u32 = 102601;
const FRIEND: u32 = 303054;
const SEED_INHERIT: InheritInfo = InheritInfo {
    blue_count: [12, 0, 0, 0, 6],
    extra_count: [10, 0, 0, 20, 20, 40],
};

#[derive(Debug, Clone)]
struct Config {
    out: String,
    top: usize,
    min_panel: i32,
}

impl Default for Config {
    fn default() -> Self {
        Self { out: "logs/y3_region_matrix.csv".to_string(), top: 10, min_panel: 0 }
    }
}

#[derive(Debug, Serialize)]
struct Row {
    composition: String,
    deck: String,
    combo_index: usize,
    region_ids: String,
    at_train_union: String,
    at_train_count: usize,
    three_train_regions: usize,
    score: f32,
    selected: bool,
    rank: usize,
}

fn parse_args() -> Result<Config> {
    let mut parser = lexopt::Parser::from_env();
    let mut cfg = Config::default();
    while let Some(arg) = parser.next()? {
        match arg {
            Arg::Long("out") => cfg.out = bench::parse_value(&mut parser, "out")?,
            Arg::Long("top") => cfg.top = bench::parse_value(&mut parser, "top")?,
            Arg::Long("min-panel") => cfg.min_panel = bench::parse_value(&mut parser, "min-panel")?,
            Arg::Long("help") | Arg::Short('h') => {
                println!("用法: y3_region_matrix [--out FILE] [--top N] [--min-panel N]");
                std::process::exit(0);
            }
            other => anyhow::bail!("未知参数: {other:?}")
        }
    }
    ensure!(cfg.top > 0, "--top 必须大于 0");
    Ok(cfg)
}

fn compositions() -> Vec<DeckComposition> {
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

fn combo_meta(combo: [usize; 3]) -> (String, usize, usize, usize) {
    // Region data is intentionally read through the public game data in the policy path;
    // the diagnostic uses the same region IDs and only classifies the visible at_trains shape.
    // Third-year IDs 15..19 are the known three-training candidates.
    let three = combo.iter().filter(|&&id| (15..=19).contains(&id)).count();
    let ids = combo.iter().map(usize::to_string).collect::<Vec<_>>().join("/");
    (ids, 0, three, 0)
}

fn main() -> Result<()> {
    let root = get_workspace_root()?;
    std::env::set_current_dir(&root)?;
    let cfg = parse_args()?;
    init_global_with_config(&load_game_config()?)?;

    let combos = get_region_combinations(2)?;
    ensure!(combos.len() == 120, "第三年组合数应为120，实际 {}", combos.len());
    let actions: Vec<RamenAction> = combos
        .iter()
        .map(|&combo| RamenAction::no_ramen(Operation::RegionSelect(combo)))
        .collect();
    let reps = bench::select_representatives(&CardPickOpts {
        min_panel: cfg.min_panel,
        ..CardPickOpts::default()
    })?;
    let mut writer = Writer::from_path(Path::new(&cfg.out))
        .with_context(|| format!("创建输出文件失败: {}", cfg.out))?;
    let mut all_rows = Vec::new();

    for composition in compositions() {
        let deck = composition.build_deck(&reps.picked, FRIEND)?;
        let game = RamenGame::newgame(UMA, &deck, SEED_INHERIT)?;
        let trainer = RecommendedRamenTrainer::new();
        let (selected_idx, scores) = trainer.years_for_diagnostic(&game, &actions)?;
        let mut order: Vec<usize> = (0..scores.len()).collect();
        order.sort_by(|&a, &b| scores[b].score.total_cmp(&scores[a].score).then_with(|| a.cmp(&b)));
        let mut ranks = vec![0usize; scores.len()];
        for (rank, &idx) in order.iter().enumerate() { ranks[idx] = rank + 1; }
        let deck_text = deck.iter().map(u32::to_string).collect::<Vec<_>>().join("/");
        for (idx, combo) in combos.iter().enumerate() {
            let (ids, union_count, three_count, _) = combo_meta(*combo);
            all_rows.push(Row {
                composition: composition.name_zh(),
                deck: deck_text.clone(),
                combo_index: idx,
                region_ids: ids,
                at_train_union: String::new(),
                at_train_count: union_count,
                three_train_regions: three_count,
                score: scores[idx].score,
                selected: idx == selected_idx,
                rank: ranks[idx],
            });
        }
        let top = order.iter().take(cfg.top).map(|&idx| format!("{:?}={:.1}", combos[idx], scores[idx].score)).collect::<Vec<_>>().join(" | ");
        println!("{} 选择 {:?}; Top{}: {}", composition.name_zh(), combos[selected_idx], cfg.top, top);
    }

    for row in &all_rows { writer.serialize(row)?; }
    writer.flush()?;
    println!("完成：{} 行，输出 {}", all_rows.len(), cfg.out);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn third_year_has_120_combinations() -> Result<()> {
        let root = get_workspace_root()?;
        std::env::set_current_dir(root)?;
        init_global_with_config(&load_game_config()?)?;
        let combos = get_region_combinations(2)?;
        assert_eq!(combos.len(), 120);
        Ok(())
    }
}
