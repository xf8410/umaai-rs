//! 拉面杯真实支援卡组合基准。
//!
//! 直接复用 [`umasim::bench::select_representatives`] 选出的五类代表卡，固定一张友人卡，
//! 枚举恰好三种普通卡类型的全部 `3-1-1` 与 `2-2-1` 真实卡牌组合；模拟前按五张普通卡
//! 的「友情+干劲+训练」面板总和，在两种构成大类中分别只保留前 10 套。它与
//! `bench_compositions` 的 101 种“类型数量构成”基准相互独立，不重复其职责。

use std::{collections::BTreeMap, path::Path};

use anyhow::{Result, ensure};
use lexopt::Arg;
use umasim::{
    bench::{self, CardPickOpts, CardRep},
    game::InheritInfo,
    gamedata::{GAMEDATA, init_global_with_config},
    trainer::{LoggingTrainer, RamenHandwrittenTrainer},
    utils::{get_workspace_root, load_game_config}
};

const DEFAULT_UMA: u32 = 102601;
const DEFAULT_FRIEND: u32 = 303054;
const DEFAULT_RUNS: usize = 300;
const DEFAULT_TOP_PER_FAMILY: usize = 10;
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

#[derive(Debug, Clone, PartialEq)]
struct ConcreteDeck {
    family: &'static str,
    composition: String,
    cards: [u32; 6],
    panel_score: f32
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

/// 解析命令行参数。
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

/// 返回一个池中不放回选取 `k` 项的全部组合。
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

/// 把五类卡数量格式化为机器可读构成名。
fn composition_name(counts: &[usize; 5]) -> String {
    counts
        .iter()
        .enumerate()
        .filter(|(_, count)| **count > 0)
        .map(|(card_type, count)| format!("{count}{}", bench::TYPE_NAMES[card_type]))
        .collect::<Vec<_>>()
        .join("+")
}

/// 读取所有满破普通 SSR 的「友情+干劲+训练」面板和，以 idrank 为键。
fn load_panel_scores() -> Result<BTreeMap<u32, f32>> {
    let data = GAMEDATA
        .get()
        .ok_or_else(|| anyhow::anyhow!("游戏数据尚未初始化"))?;
    Ok(data
        .card
        .values()
        .filter(|card| card.rarity == 3 && (0..5).contains(&card.card_type) && card.card_value.len() >= 5)
        .map(|card| {
            let value = &card.card_value[4];
            (
                card.card_id * 10 + 4,
                value.youqing + value.ganjing as f32 + value.xunlian as f32
            )
        })
        .collect())
}

/// 将一套五张普通卡追加固定友人，并计算普通卡面板总和。
fn push_deck(
    result: &mut Vec<ConcreteDeck>, ids: Vec<u32>, counts: [usize; 5], family: &'static str,
    friend: u32, panel_scores: &BTreeMap<u32, f32>
) -> Result<()> {
    let panel_score = ids.iter().try_fold(0.0, |sum, id| {
        panel_scores
            .get(id)
            .map(|score| sum + score)
            .ok_or_else(|| anyhow::anyhow!("卡 {id} 缺少满破面板数据"))
    })?;
    let mut cards = ids;
    cards.push(friend);
    let cards: [u32; 6] = cards
        .try_into()
        .map_err(|_| anyhow::anyhow!("枚举器必须生成五张普通卡"))?;
    result.push(ConcreteDeck {
        family,
        composition: composition_name(&counts),
        cards,
        panel_score
    });
    Ok(())
}

/// 枚举恰好三种普通卡类型的 3-1-1 与 2-2-1 真实卡组。
fn enumerate_concrete_decks(
    pools: &[Vec<u32>; 5], friend: u32, panel_scores: &BTreeMap<u32, f32>
) -> Result<Vec<ConcreteDeck>> {
    let mut result = Vec::new();
    for first in 0..5 {
        for second in first + 1..5 {
            for third in second + 1..5 {
                let types = [first, second, third];

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
                                push_deck(&mut result, ids, counts, "3-1-1", friend, panel_scores)?;
                            }
                        }
                    }
                }

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
                                push_deck(&mut result, ids, counts, "2-2-1", friend, panel_scores)?;
                            }
                        }
                    }
                }
            }
        }
    }
    Ok(result)
}

/// 在 3-1-1 与 2-2-1 两个大类中分别保留面板总和最高的 `limit` 套。
fn select_top_by_family(decks: &[ConcreteDeck], limit: usize) -> Vec<ConcreteDeck> {
    let mut selected = Vec::with_capacity(limit * 2);
    for family in ["3-1-1", "2-2-1"] {
        let mut family_decks: Vec<ConcreteDeck> = decks
            .iter()
            .filter(|deck| deck.family == family)
            .cloned()
            .collect();
        family_decks.sort_by(|left, right| {
            right
                .panel_score
                .total_cmp(&left.panel_score)
                .then_with(|| left.cards.cmp(&right.cards))
        });
        family_decks.truncate(limit);
        selected.extend(family_decks);
    }
    selected
}

/// 使用当前上游固定种子入口运行一套卡组。
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

/// 将模拟汇总写入 CSV。
fn write_results(path: &Path, summaries: &[Summary]) -> Result<()> {
    let rows: Vec<Vec<String>> = summaries
        .iter()
        .map(|summary| {
            vec![
                summary.deck.family.to_string(),
                summary.deck.composition.clone(),
                format!("{:.3}", summary.deck.panel_score),
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
            "family",
            "composition",
            "panel_score",
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

/// 运行真实支援卡组合基准。
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

    let panel_scores = load_panel_scores()?;
    let all_decks = enumerate_concrete_decks(&pools, cfg.friend, &panel_scores)?;
    let selected_decks = select_top_by_family(&all_decks, DEFAULT_TOP_PER_FAMILY);
    let decks: Vec<ConcreteDeck> = selected_decks
        .iter()
        .enumerate()
        .filter(|(index, _)| index % cfg.shard_count == cfg.shard_index)
        .map(|(_, deck)| deck.clone())
        .collect();
    println!(
        "候选={} 面板筛选后={}（3-1-1/2-2-1 各最多{}）当前分片={}/{} 卡组数={} 每套局数={} 总局数={}",
        all_decks.len(),
        selected_decks.len(),
        DEFAULT_TOP_PER_FAMILY,
        cfg.shard_index,
        cfg.shard_count,
        decks.len(),
        cfg.runs,
        decks.len() * cfg.runs
    );
    for deck in &selected_decks {
        println!(
            "入选 {} {} panel={:.1} {:?}",
            deck.family, deck.composition, deck.panel_score, deck.cards
        );
    }
    if cfg.enumerate_only {
        return Ok(());
    }

    let deck_count = decks.len();
    let mut summaries = Vec::with_capacity(deck_count);
    for (index, deck) in decks.into_iter().enumerate() {
        eprintln!(
            "[{}/{}] {} {} panel={:.1} {:?}",
            index + 1,
            deck_count,
            deck.family,
            deck.composition,
            deck.panel_score,
            deck.cards
        );
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

    /// 构造每类三张的测试卡池。
    fn three_card_pools() -> [Vec<u32>; 5] {
        std::array::from_fn(|card_type| {
            (0..3)
                .map(|index| 100_000 + card_type as u32 * 10 + index)
                .collect()
        })
    }

    /// 构造可预测排序的测试面板分数。
    fn test_panel_scores(pools: &[Vec<u32>; 5]) -> BTreeMap<u32, f32> {
        pools
            .iter()
            .flatten()
            .map(|id| (*id, (*id - 100_000) as f32))
            .collect()
    }

    #[test]
    fn three_cards_per_type_generate_1080_unique_decks() -> Result<()> {
        let pools = three_card_pools();
        let decks = enumerate_concrete_decks(&pools, DEFAULT_FRIEND, &test_panel_scores(&pools))?;
        ensure!(decks.len() == 1080, "应生成 1080 套，实际 {}", decks.len());
        let keys: HashSet<Vec<u32>> = decks
            .iter()
            .map(|deck| {
                let mut key = deck.cards[..5].to_vec();
                key.sort_unstable();
                key
            })
            .collect();
        ensure!(keys.len() == decks.len(), "生成了重复卡组");
        ensure!(
            decks.iter().all(|deck| deck.cards[5] == DEFAULT_FRIEND),
            "固定友人槽错误"
        );
        println!("候选卡组={}，去重后={}", decks.len(), keys.len());
        Ok(())
    }

    #[test]
    fn panel_filter_keeps_ten_per_family() -> Result<()> {
        let pools = three_card_pools();
        let decks = enumerate_concrete_decks(&pools, DEFAULT_FRIEND, &test_panel_scores(&pools))?;
        let selected = select_top_by_family(&decks, 10);
        let count_311 = selected.iter().filter(|deck| deck.family == "3-1-1").count();
        let count_221 = selected.iter().filter(|deck| deck.family == "2-2-1").count();
        ensure!(count_311 == 10, "3-1-1 应保留 10 套，实际 {count_311}");
        ensure!(count_221 == 10, "2-2-1 应保留 10 套，实际 {count_221}");
        for family in ["3-1-1", "2-2-1"] {
            let family_decks: Vec<&ConcreteDeck> = selected.iter().filter(|deck| deck.family == family).collect();
            ensure!(
                family_decks
                    .windows(2)
                    .all(|pair| pair[0].panel_score >= pair[1].panel_score),
                "{family} 未按面板总和降序"
            );
        }
        println!("筛选后总数={}：3-1-1={count_311}，2-2-1={count_221}", selected.len());
        Ok(())
    }

    #[test]
    fn generated_decks_are_only_311_or_221() -> Result<()> {
        let pools = three_card_pools();
        let decks = enumerate_concrete_decks(&pools, DEFAULT_FRIEND, &test_panel_scores(&pools))?;
        for deck in decks {
            let mut counts = [0; 5];
            for id in &deck.cards[..5] {
                let card_type = ((*id - 100_000) / 10) as usize;
                counts[card_type] += 1;
            }
            let mut nonzero: Vec<usize> = counts.into_iter().filter(|count| *count > 0).collect();
            nonzero.sort_unstable();
            ensure!(
                nonzero == [1, 1, 3] || nonzero == [1, 2, 2],
                "生成了非法构成: {nonzero:?}"
            );
        }
        println!("全部候选均为 3-1-1 或 2-2-1");
        Ok(())
    }

    #[test]
    fn combinations_are_without_replacement() -> Result<()> {
        let actual = combinations(&[1, 2, 3, 4], 3);
        let expected = vec![vec![1, 2, 3], vec![1, 2, 4], vec![1, 3, 4], vec![2, 3, 4]];
        ensure!(actual == expected, "组合枚举错误: {actual:?}");
        println!("不放回组合: {actual:?}");
        Ok(())
    }
}
