//! 拉面杯真实支援卡组合基准。
//!
//! 直接复用 [`umasim::bench::select_representatives`] 选出的五类代表卡，固定一张友人卡，
//! 枚举恰好三种普通卡类型的全部 `3-1-1` 与 `2-2-1` 真实卡牌组合。它与
//! `bench_compositions` 的 101 种“类型数量构成”基准相互独立，不重复其职责。

use std::path::Path;

use anyhow::{Result, ensure};
use lexopt::Arg;
use umasim::{
    bench::{self, CardPickOpts, CardRep},
    game::InheritInfo,
    gamedata::{init_global_with_config},
    trainer::{LoggingTrainer, RamenHandwrittenTrainer},
    utils::{get_workspace_root, load_game_config}
};

const DEFAULT_UMA: u32 = 102601;
const DEFAULT_FRIEND: u32 = 303054;
const DEFAULT_RUNS: usize = 300;
const DEFAULT_INHERIT: InheritInfo = InheritInfo {
    blue_count: [12, 0, 0, 0, 6],
    extra_count: [10, 0, 0, 20, 20, 40]
};

#[derive(Debug, Clone)]
struct Config {
    runs: usize,
    seed: u64,
    friend: u32,
    pick: CardPickOpts,
    shard_index: usize,
    shard_count: usize,
    enumerate_only: bool,
    out: String
}

impl Default for Config {
    fn default() -> Self {
        Self {
            runs: DEFAULT_RUNS,
            seed: 42,
            friend: DEFAULT_FRIEND,
            pick: CardPickOpts::default(),
            shard_index: 0,
            shard_count: 1,
            enumerate_only: false,
            out: "logs/bench_card_combinations.csv".to_string()
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ConcreteDeck {
    composition: String,
    cards: [u32; 6]
}

#[derive(Debug)]
struct Summary {
    deck: ConcreteDeck,
    completed: usize,
    failed: usize,
    score_mean: f64,
    score_median: f64,
    score_p10: f64
}

fn parse_args() -> Result<Config> {
    let mut parser = lexopt::Parser::from_env();
    let mut cfg = Config::default();
    while let Some(arg) = parser.next()? {
        match arg {
            Arg::Long("runs") => cfg.runs = bench::parse_value(&mut parser, "runs")?,
            Arg::Long("seed") => cfg.seed = bench::parse_value(&mut parser, "seed")?,
            Arg::Long("friend") => cfg.friend = bench::parse_value(&mut parser, "friend")?,
            Arg::Long("min-panel") => cfg.pick.min_panel = bench::parse_value(&mut parser, "min-panel")?,
            Arg::Long("pool-size") => cfg.pick.pool_size = bench::parse_value(&mut parser, "pool-size")?,
            Arg::Long("pick") => cfg.pick.pick = bench::parse_value(&mut parser, "pick")?,
            Arg::Long("shard-index") => cfg.shard_index = bench::parse_value(&mut parser, "shard-index")?,
            Arg::Long("shard-count") => cfg.shard_count = bench::parse_value(&mut parser, "shard-count")?,
            Arg::Long("out") => cfg.out = bench::parse_value(&mut parser, "out")?,
            Arg::Long("enumerate-only") => cfg.enumerate_only = true,
            Arg::Long("help") | Arg::Short('h') => {
                println!(
                    "用法: bench_card_combinations [--runs N] [--seed S] [--friend IDRANK] \
                     [--min-panel N] [--pool-size N] [--pick N] \
                     [--shard-index N] [--shard-count N] [--enumerate-only] [--out FILE]"
                );
                std::process::exit(0);
            }
            other => anyhow::bail!("未知参数: {other:?}（可用 --help 查看用法）")
        }
    }
    ensure!(cfg.runs > 0, "--runs 必须大于 0");
    ensure!(cfg.pick.pick >= 3, "--pick 必须至少为 3，才能生成 3-1-1 组合");
    ensure!(cfg.shard_count > 0, "--shard-count 必须大于 0");
    ensure!(
        cfg.shard_index < cfg.shard_count,
        "--shard-index 必须小于 --shard-count"
    );
    Ok(cfg)
}

fn combinations(pool: &[u32], k: usize) -> Vec<Vec<u32>> {
    if k == 0 || k > pool.len() {
        return Vec::new();
    }
    let mut result = Vec::new();
    let mut indices: Vec<usize> = (0..k).collect();
    'outer: loop {
        result.push(indices.iter().map(|&index| pool[index]).collect());
        let mut index = k;
        loop {
            if index == 0 {
                break 'outer;
            }
            index -= 1;
            if indices[index] < pool.len() - (k - index) {
                indices[index] += 1;
                for next in index + 1..k {
                    indices[next] = indices[next - 1] + 1;
                }
                break;
            }
        }
    }
    result
}

fn composition_name(counts: &[usize; 5]) -> String {
    counts
        .iter()
        .enumerate()
        .filter(|(_, count)| **count > 0)
        .map(|(card_type, count)| format!("{count}{}", bench::TYPE_NAMES[card_type]))
        .collect::<Vec<_>>()
        .join("+")
}

fn push_deck(result: &mut Vec<ConcreteDeck>, ids: Vec<u32>, counts: [usize; 5], friend: u32) {
    let mut cards = ids;
    cards.push(friend);
    result.push(ConcreteDeck {
        composition: composition_name(&counts),
        cards: cards.try_into().expect("枚举器必须生成五张普通卡")
    });
}

/// 枚举恰好三种普通卡类型的 3-1-1 与 2-2-1 真实卡组。
fn enumerate_concrete_decks(pools: &[Vec<u32>; 5], friend: u32) -> Vec<ConcreteDeck> {
    let mut result = Vec::new();
    for first in 0..5 {
        for second in first + 1..5 {
            for third in second + 1..5 {
                let types = [first, second, third];

                // 3-1-1：依次指定三张卡所属类型。
                for &triple_type in &types {
                    let others: Vec<usize> = types
                        .iter()
                        .copied()
                        .filter(|card_type| *card_type != triple_type)
                        .collect();
                    for triple in combinations(&pools[triple_type], 3) {
                        for &left in &pools[others[0]] {
                            for &right in &pools[others[1]] {
                                let mut ids = triple.clone();
                                ids.push(left);
                                ids.push(right);
                                let mut counts = [0; 5];
                                counts[triple_type] = 3;
                                counts[others[0]] = 1;
                                counts[others[1]] = 1;
                                push_deck(&mut result, ids, counts, friend);
                            }
                        }
                    }
                }

                // 2-2-1：依次指定单张卡所属类型。
                for &single_type in &types {
                    let pairs: Vec<usize> = types
                        .iter()
                        .copied()
                        .filter(|card_type| *card_type != single_type)
                        .collect();
                    for left_pair in combinations(&pools[pairs[0]], 2) {
                        for right_pair in combinations(&pools[pairs[1]], 2) {
                            for &single in &pools[single_type] {
                                let mut ids = left_pair.clone();
                                ids.extend_from_slice(&right_pair);
                                ids.push(single);
                                let mut counts = [0; 5];
                                counts[pairs[0]] = 2;
                                counts[pairs[1]] = 2;
                                counts[single_type] = 1;
                                push_deck(&mut result, ids, counts, friend);
                            }
                        }
                    }
                }
            }
        }
    }
    result
}

fn run_deck(cfg: &Config, deck: ConcreteDeck) -> Summary {
    let mut scores = Vec::with_capacity(cfg.runs);
    let mut failed = 0;
    for run_index in 0..cfg.runs {
        let seed = cfg.seed + run_index as u64;
        let mut trainer = LoggingTrainer::new(RamenHandwrittenTrainer::new(), seed);
        trainer.set_logging(false);
        match bench::run_seeded(DEFAULT_UMA, &deck.cards, &DEFAULT_INHERIT, seed, &trainer) {
            Ok(outcome) => scores.push(f64::from(outcome.score)),
            Err(error) => {
                failed += 1;
                eprintln!("卡组 {:?} seed={seed} 模拟失败: {error:#}", deck.cards);
            }
        }
    }
    scores.sort_by(f64::total_cmp);
    let stats = bench::summarize(&scores);
    Summary {
        deck,
        completed: scores.len(),
        failed,
        score_mean: stats.mean,
        score_median: stats.median,
        score_p10: bench::percentile(&scores, 0.1)
    }
}

fn write_results(path: &Path, summaries: &[Summary]) -> Result<()> {
    let rows: Vec<Vec<String>> = summaries
        .iter()
        .map(|summary| {
            vec![
                summary.composition.clone(),
                summary
                    .deck
                    .cards
                    .iter()
                    .map(u32::to_string)
                    .collect::<Vec<_>>()
                    .join("/"),
                summary.completed.to_string(),
                summary.failed.to_string(),
                format!("{:.3}", summary.score_mean),
                format!("{:.3}", summary.score_median),
                format!("{:.3}", summary.score_p10)
            ]
        })
        .collect();
    bench::write_csv(
        path,
        &[
            "composition",
            "deck",
            "completed",
            "failed",
            "score_mean",
            "score_median",
            "score_p10"
        ],
        &rows
    )
}

fn main() -> Result<()> {
    let workspace_root = get_workspace_root()?;
    std::env::set_current_dir(&workspace_root)?;
    let cfg = parse_args()?;
    init_global_with_config(&load_game_config()?)?;

    let selected = bench::select_representatives(&cfg.pick)?;
    let pools: [Vec<u32>; 5] =
        std::array::from_fn(|card_type| selected.picked[card_type].iter().map(|card| card.idrank).collect());
    for (card_type, cards) in selected.picked.iter().enumerate() {
        println!(
            "{}: {}",
            bench::type_name_zh(card_type),
            cards
                .iter()
                .map(|card: &CardRep| format!("{} {}", card.idrank, card.name))
                .collect::<Vec<_>>()
                .join(" / ")
        );
    }

    let all_decks = enumerate_concrete_decks(&pools, cfg.friend);
    let decks: Vec<ConcreteDeck> = all_decks
        .iter()
        .enumerate()
        .filter(|(index, _)| index % cfg.shard_count == cfg.shard_index)
        .map(|(_, deck)| deck.clone())
        .collect();
    println!(
        "真实卡组总数={} 当前分片={}/{} 卡组数={} 每套局数={} 总局数={}",
        all_decks.len(),
        cfg.shard_index,
        cfg.shard_count,
        decks.len(),
        cfg.runs,
        decks.len() * cfg.runs
    );
    if cfg.enumerate_only {
        return Ok(());
    }

    let mut summaries = Vec::with_capacity(decks.len());
    for (index, deck) in decks.into_iter().enumerate() {
        eprintln!("[{}/{}] {} {:?}", index + 1, summaries.capacity(), deck.composition, deck.cards);
        summaries.push(run_deck(&cfg, deck));
    }
    summaries.sort_by(|left, right| right.score_mean.total_cmp(&left.score_mean));
    write_results(Path::new(&cfg.out), &summaries)?;
    let failed: usize = summaries.iter().map(|summary| summary.failed).sum();
    ensure!(failed == 0, "存在 {failed} 个异常局，结果不完整");
    println!("完成：{}，结果写入 {}", summaries.len(), cfg.out);
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;

    fn three_card_pools() -> [Vec<u32>; 5] {
        std::array::from_fn(|card_type| {
            (0..3)
                .map(|index| 100_000 + card_type as u32 * 10 + index)
                .collect()
        })
    }

    #[test]
    fn three_cards_per_type_generate_1080_unique_decks() {
        let decks = enumerate_concrete_decks(&three_card_pools(), DEFAULT_FRIEND);
        assert_eq!(decks.len(), 1080);
        let keys: HashSet<Vec<u32>> = decks
            .iter()
            .map(|deck| {
                let mut key = deck.cards[..5].to_vec();
                key.sort_unstable();
                key
            })
            .collect();
        assert_eq!(keys.len(), decks.len());
        assert!(decks.iter().all(|deck| deck.cards[5] == DEFAULT_FRIEND));
    }

    #[test]
    fn generated_decks_are_only_311_or_221() {
        let pools = three_card_pools();
        for deck in enumerate_concrete_decks(&pools, DEFAULT_FRIEND) {
            let mut counts = [0; 5];
            for id in &deck.cards[..5] {
                let card_type = ((*id - 100_000) / 10) as usize;
                counts[card_type] += 1;
            }
            let mut nonzero: Vec<usize> = counts.into_iter().filter(|count| *count > 0).collect();
            nonzero.sort_unstable();
            assert!(nonzero == [1, 1, 3] || nonzero == [1, 2, 2]);
        }
    }

    #[test]
    fn combinations_are_without_replacement() {
        assert_eq!(
            combinations(&[1, 2, 3, 4], 3),
            vec![vec![1, 2, 3], vec![1, 2, 4], vec![1, 3, 4], vec![2, 3, 4]]
        );
    }
}
