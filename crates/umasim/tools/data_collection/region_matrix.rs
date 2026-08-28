//! 地区选择诊断工具：对每个 build 跑 N 局，打出 build 配置和三年的地区选择（含地区名）。
//!
//! 卡组来源于 `bench_config.toml` 的 `[player_builds]`（7 个 build）；
//! 抽样列表由 `SAMPLE_BUILDS=name1,name2,...` 控制（按 build 名选，未设时取默认
//! `speed,stamina,power_wisdom,spd2_gut0`——覆盖速/耐/智/速力四类卡组倾向）。
//! `ALL_COMPOSITIONS=1` 时跑全 101 种合法卡组构成（5×0..3 枚举）。
//!
//! 每 build 内部跑 N 局（默认 20），同种子下记录每年地区选择。
//! 跨 build 一致性：同一 build 在 20 局里选同一地区组合的局数。
//!
//! 输出：
//! - CSV（每行一局）：build, run_idx, y1, y2, y3（地区 id）+ 三个总分
//! - 终端：每个 build 的"build 配置 + 三年的地区选择（含名）"

use std::{env, fs::File, io::Write};

use anyhow::{Context, Result};
use umasim::{
    bench::{self, DeckComposition, GameOutcome},
    game::InheritInfo,
    gamedata::{init_global_with_config, ramen::RAMENDATA},
    trainer::{LoggingTrainer, RecommendedRamenTrainer},
    utils::{get_workspace_root, load_game_config}
};

const BASE_SEED: u64 = 61_444;
const UMA: u32 = 102_601;
/// 友人卡（满破 idrank=303054），与 `bench_compositions` 默认一致。
const FRIEND: u32 = 303054;
/// 继承因子，与 `bench_compositions` 默认一致（[12,0,0,0,6] / [10,0,0,20,20,40]）。
const INHERIT: InheritInfo = InheritInfo {
    blue_count: [12, 0, 0, 0, 6],
    extra_count: [10, 0, 0, 20, 20, 40]
};
/// 首轮默认抽样 build（速/耐/智/速力四类卡组倾向对照）。
const DEFAULT_SAMPLE_BUILDS: &[&str] = &["speed", "stamina", "power_wisdom", "spd2_gut0"];

/// 固定地区选择训练员：RegionSelect 候选时强制返回指定组合，其余决策代理内层策略。
///
/// 用于 `FIXED_AB=1` 模式：固定地区 A/B 对照，隔离地区选择对终局评分的影响。
/// 内层含 Mutex 不可 Clone，故整局对比时每局重建。
struct FixedRegionTrainer {
    inner: umasim::trainer::RecommendedRamenTrainer,
    fixed: [[usize; 3]; 3]
}

impl umasim::game::Trainer<umasim::game::ramen::RamenGame> for FixedRegionTrainer {
    fn select_action(
        &self,
        game: &umasim::game::ramen::RamenGame,
        actions: &[umasim::game::ramen::RamenAction],
        rng: &mut rand::prelude::StdRng
    ) -> Result<usize> {
        use umasim::game::{Game, ramen::Operation};
        if actions.iter().any(|a| matches!(a.operation, Operation::RegionSelect(_))) {
            let year = match game.turn() {
                2 => 0,
                23 => 1,
                47 => 2,
                _ => 0
            };
            let mut combo = self.fixed[year];
            combo.sort(); // 候选为排序组合（get_region_combinations），容忍常量乱序
            return actions
                .iter()
                .position(|a| matches!(a.operation, Operation::RegionSelect(c) if c == combo))
                .ok_or_else(|| anyhow::anyhow!("固定组合不在候选: year={year} combo={combo:?}"));
        }
        self.inner.select_action(game, actions, rng)
    }

    fn select_choice(
        &self,
        game: &umasim::game::ramen::RamenGame,
        choices: &[Vec<umasim::gamedata::EventChoice>],
        rng: &mut rand::prelude::StdRng
    ) -> Result<usize> {
        self.inner.select_choice(game, choices, rng)
    }

    fn select_event_choice(
        &self,
        game: &umasim::game::ramen::RamenGame,
        event: &umasim::gamedata::EventData,
        choices: &[Vec<umasim::gamedata::EventChoice>],
        rng: &mut rand::prelude::StdRng
    ) -> Result<usize> {
        self.inner.select_event_choice(game, event, choices, rng)
    }
}

/// 解析抽样 build 名列表：未设 `SAMPLE_BUILDS` 时取 [`DEFAULT_SAMPLE_BUILDS`]；
/// 否则按逗号分隔、按 `[player_builds]` 里的 `name` 反查，缺失即报错。
///
/// 按 build 名（而非按声明下步长）抽样，是为了让任意子集都能稳定选中（步长抽样
/// 受声明序影响，跳着抽不到特定 build）。
fn parse_sample_builds(all_builds: &[DeckComposition]) -> Result<Vec<DeckComposition>> {
    let names: Vec<String> = match env::var("SAMPLE_BUILDS") {
        Ok(s) if !s.trim().is_empty() => s.split(',').map(|p| p.trim().to_string()).collect(),
        _ => DEFAULT_SAMPLE_BUILDS.iter().map(|s| s.to_string()).collect()
    };
    let mut result = Vec::with_capacity(names.len());
    for name in &names {
        let build = all_builds
            .iter()
            .find(|b| b.name == *name)
            .with_context(|| format!("SAMPLE_BUILDS 中的 '{name}' 不在 bench_config.toml [player_builds] 中"))?;
        result.push(build.clone());
    }
    Ok(result)
}

/// 枚举全部 101 种合法普通卡类型构成（速/耐/力/根/智各 0..=3 张、合计 5 张）。
///
/// 与 `composition_weight_matrix` 内部实现一致——就地复制避免跨 crate 依赖。
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

/// 把 build counts 渲染成人类可读字符串（如 `3speed+2wisdom`）。
fn build_display(counts: &[usize; 5]) -> String {
    const NAMES: [&str; 5] = ["speed", "stamina", "power", "guts", "wisdom"];
    let mut parts = Vec::with_capacity(5);
    for (i, &c) in counts.iter().enumerate() {
        if c > 0 {
            parts.push(format!("{c}{}", NAMES[i]));
        }
    }
    parts.join("+")
}

/// 把三个地区 id 渲染成 `id1 (name1)/id2 (name2)/id3 (name3)` 形式。
fn region_combo_display(combo: &[usize; 3]) -> String {
    let ramen_data = RAMENDATA.get().expect("RAMENDATA 未初始化");
    let names: Vec<String> = combo
        .iter()
        .map(|&rid| {
            ramen_data
                .ramen_region_effect
                .get(rid)
                .map(|r| format!("{rid} ({})", r.name))
                .unwrap_or_else(|| format!("{rid} (?)"))
        })
        .collect();
    names.join("/")
}

fn region_cell(out: &GameOutcome, y: usize) -> String {
    let r = out.yearly_selected_regions[y];
    format!("{}/{}/{}", r[0], r[1], r[2])
}

fn main() -> Result<()> {
    std::env::set_current_dir(get_workspace_root()?)?;
    init_global_with_config(&load_game_config()?)?;

    // 固定地区对比模式：用户给定的 A/B 三年地区方案（speed build）。整局同种子对比，
    // 其余决策一致，只差每年地区——地区选择对终局评分的唯一可信验证手段。
    if env::var("FIXED_AB").is_ok_and(|v| v == "1") {
        const GROUP_A: [[usize; 3]; 3] = [[0, 1, 4], [5, 7, 9], [11, 14, 17]];
        const GROUP_B: [[usize; 3]; 3] = [[1, 0, 3], [6, 7, 9], [11, 14, 15]];
        let runs: u64 = env::var("RUNS").unwrap_or_else(|_| "20".into()).parse()?;
        let all_builds = bench::load_player_builds()?;
        // FIXED_AB 模式固定走 speed build（与原硬编码 DECK 对齐）。
        let deck = all_builds
            .iter()
            .find(|b| b.name == "speed")
            .with_context(|| "FIXED_AB 模式需要 speed build 存在")?
            .make_deck(&bench::CardPickOpts::default(), FRIEND)?;
        for (name, g) in [("A", GROUP_A), ("B", GROUP_B)] {
            let mut sum = 0f64;
            let mut gains = [0f64; 3];
            let mut overs = [0f64; 3];
            for run_idx in 0..runs {
                let trainer = LoggingTrainer::new(
                    FixedRegionTrainer { inner: RecommendedRamenTrainer::new(), fixed: g },
                    run_idx
                );
                let o = bench::run_seeded(UMA, &deck, &INHERIT, BASE_SEED, run_idx, &trainer)?;
                sum += o.score as f64;
                for y in 0..3 {
                    gains[y] += o.yearly_gauge_gain[y] as f64;
                    overs[y] += o.yearly_gauge_overflow[y] as f64;
                }
            }
            println!(
                "{name} 固定地区整局: 平均分={:.0} ({runs} 局) 诀窍获得={:.1}/{:.1}/{:.1} 溢出={:.1}/{:.1}/{:.1}",
                sum / runs as f64,
                gains[0] / runs as f64,
                gains[1] / runs as f64,
                gains[2] / runs as f64,
                overs[0] / runs as f64,
                overs[1] / runs as f64,
                overs[2] / runs as f64
            );
        }
        return Ok(());
    }

    let runs: u64 = env::var("RUNS").unwrap_or_else(|_| "20".into()).parse()?;

    // 多 build 抽样：默认走 [player_builds] 按 SAMPLE_BUILDS 名选；ALL_COMPOSITIONS=1 走全 101 种
    let all_compositions = env::var("ALL_COMPOSITIONS").is_ok_and(|v| v == "1");
    let sample: Vec<DeckComposition> = if all_compositions {
        let mut comps = enumerate_compositions();
        anyhow::ensure!(
            comps.len() == 101,
            "全枚举构成数量应为 101，实际为 {}",
            comps.len()
        );
        comps.sort_by_key(|c| c.counts);
        comps
    } else {
        let all_builds = bench::load_player_builds()?;
        parse_sample_builds(&all_builds)?
    };
    anyhow::ensure!(!sample.is_empty(), "抽样 build 列表为空");

    // CSV 文件名：未设 OUT_PREFIX 时用默认；设了则自定义。
    // 一律写到 `logs/` 下——与 bench_base / bench_compositions 输出口径一致，
    // 避免 workspace 根目录被诊断数据污染。
    let csv_path = match env::var("OUT_PREFIX") {
        Ok(prefix) if !prefix.is_empty() => format!("logs/{prefix}.csv"),
        _ => "logs/region-matrix-result.csv".to_string()
    };
    let mut out = File::create(&csv_path)?;
    println!("CSV: {csv_path}");
    writeln!(out, "build,run_idx,score,y1,y2,y3")?;

    // 跨 build 一致性：每 build 的"每一年最常见地区组合"及其占比
    // 最常见地区组合按年份分别追踪——`most_by_year[y]` = 该 build 在第 y 年
    // 选得最多的 3 个地区 id
    let mut per_build_summary: Vec<(DeckComposition, usize, [[usize; 3]; 3], [usize; 3])> = Vec::new();

    for build in &sample {
        let deck = build.make_deck(&bench::CardPickOpts::default(), FRIEND)?;
        // 每 build 内按年份分别记录：每个地区组合出现的局数
        let mut combo_counts_by_year: [std::collections::HashMap<[usize; 3], usize>; 3] = [
            std::collections::HashMap::new(),
            std::collections::HashMap::new(),
            std::collections::HashMap::new(),
        ];
        for run_idx in 0..runs {
            // RecommendedRamenTrainer 含 Mutex 不可 Clone，每局重建（构造开销可忽略）
            let outcome = bench::run_seeded(
                UMA, &deck, &INHERIT, BASE_SEED, run_idx,
                &LoggingTrainer::new(RecommendedRamenTrainer::new(), run_idx)
            )?;
            let combos = outcome.yearly_selected_regions;
            for y in 0..3 {
                *combo_counts_by_year[y].entry(combos[y]).or_insert(0) += 1;
            }

            writeln!(
                out,
                "{},{},{},{},{},{}",
                build.name(),
                run_idx,
                outcome.score,
                region_cell(&outcome, 0),
                region_cell(&outcome, 1),
                region_cell(&outcome, 2)
            )?;
        }
        // 每年找最常见的地区组合
        let mut most_by_year = [[0usize; 3]; 3];
        let mut most_count_by_year = [0usize; 3];
        for y in 0..3 {
            if let Some((combo, count)) = combo_counts_by_year[y]
                .iter()
                .max_by_key(|(_, c)| *c)
            {
                most_by_year[y] = *combo;
                most_count_by_year[y] = *count;
            }
        }
        per_build_summary.push((build.clone(), runs as usize, most_by_year, most_count_by_year));
    }

    // 终端：每个 build 的"build 配置 + 三年的地区选择"
    println!(
        "\n=== 地区选择诊断：跨 {} build × {} 局共 {} 局 ===",
        per_build_summary.len(),
        runs,
        per_build_summary.len() * runs as usize
    );
    for (build, n_runs, most_by_year, most_counts) in &per_build_summary {
        let y1 = most_by_year[0];
        let y2 = most_by_year[1];
        let y3 = most_by_year[2];
        let y1_pct = most_counts[0] as f64 / *n_runs as f64 * 100.0;
        let y2_pct = most_counts[1] as f64 / *n_runs as f64 * 100.0;
        let y3_pct = most_counts[2] as f64 / *n_runs as f64 * 100.0;
        println!(
            "\n  build={} ({})\n    Y1 = {} ({}/{}={:.0}%)\n    Y2 = {} ({}/{}={:.0}%)\n    Y3 = {} ({}/{}={:.0}%)",
            build_display(&build.counts),
            build.name(),
            region_combo_display(&y1),
            most_counts[0], n_runs, y1_pct,
            region_combo_display(&y2),
            most_counts[1], n_runs, y2_pct,
            region_combo_display(&y3),
            most_counts[2], n_runs, y3_pct,
        );
    }

    // CSV 汇总（追加）
    println!("\nCSV: {csv_path}");
    Ok(())
}