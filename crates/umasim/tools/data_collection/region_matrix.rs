//! 地区选择组合级指标矩阵（最小实验）：同种子配对正式推荐策略（a）与地区指标变体（b）。
//!
//! 目前仅有 `low_count_youqing` 一个组合级指标（经固定地区整局对比验证有区分度；
//! 诀窍模拟类指标全部弃用，见 issues.md）。`VARIANT` token 串（短横线分隔，缺省取 0）：
//! ```text
//! lowc<v>    → region_low_count_youqing_weight = v/100（卡少 youqing 加权）
//! lowk<v>    → region_low_count_k = v                 （放大倍率）
//! ```
//! 空串（`VARIANT=plain`）为全 0 对照，应逐位复现 a（自检用）。
//!
//! CSV 输出到 `region-matrix-result.csv`：score 差 + 三年地区选择 + 第 3 年诀窍获得/溢出。
//! `FIXED_AB=1` 为固定地区方案整局对比（用户给定 A/B 三年组合，同种子，其余决策一致，
//! 是地区选择的唯一可信验证手段）。

use std::{env, fs::File, io::Write};

use anyhow::{Context, Result};
use umasim::{
    bench::{self, GameOutcome},
    game::InheritInfo,
    gamedata::init_global_with_config,
    trainer::{LoggingTrainer, RecommendedRamenTrainer},
    utils::{get_workspace_root, load_game_config}
};

const BASE_SEED: u64 = 61_444;
const UMA: u32 = 102_601;
const DECK: [u32; 6] = [302424, 302894, 303044, 302924, 303024, 303054];
const INHERIT: InheritInfo = InheritInfo {
    blue_count: [15, 0, 0, 0, 3],
    extra_count: [10, 10, 20, 20, 20, 40]
};

/// 固定地区选择训练员：RegionSelect 候选时强制返回指定组合，其余决策代理内层策略。
///
/// 用于「固定地区方案对比」：同样的策略在其他决策点上一致，只差每年地区（隔离
/// 地区选择对终局评分的影响）。内层含 Mutex 不可 Clone，故整局对比时每局重建。
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

/// 解析 VARIANT token 串，返回 `with_region_overrides` 的两个参数。
fn parse_variant(name: &str) -> Result<(f32, f32)> {
    let mut lowc = 0.0f32;
    let mut lowk = 2.0f32;
    for token in name.split('-') {
        if token.is_empty() || token == "plain" {
            continue;
        }
        if let Some(v) = token.strip_prefix("lowc") {
            lowc = v.parse::<f32>()? / 100.0;
        } else if let Some(v) = token.strip_prefix("lowk") {
            lowk = v.parse::<f32>()?;
        } else {
            anyhow::bail!("未知地区变体字段: {token} ({name})");
        }
    }
    Ok((lowc, lowk))
}

fn region_cell(out: &GameOutcome, y: usize) -> String {
    let r = out.yearly_selected_regions[y];
    format!("{}/{}/{}", r[0], r[1], r[2])
}

fn main() -> Result<()> {
    std::env::set_current_dir(get_workspace_root()?)?;
    init_global_with_config(&load_game_config()?)?;

    // 固定地区对比模式：用户给定的 A/B 三年地区方案（3 速 build）。整局同种子对比，
    // 其余决策一致，只差每年地区——地区选择对终局评分的唯一可信验证手段。
    if env::var("FIXED_AB").is_ok_and(|v| v == "1") {
        const GROUP_A: [[usize; 3]; 3] = [[0, 1, 4], [5, 7, 9], [11, 14, 17]];
        const GROUP_B: [[usize; 3]; 3] = [[1, 0, 3], [6, 7, 9], [11, 14, 15]];
        let runs: u64 = env::var("RUNS").unwrap_or_else(|_| "20".into()).parse()?;
        for (name, g) in [("A", GROUP_A), ("B", GROUP_B)] {
            let mut sum = 0f64;
            let mut gains = [0f64; 3];
            let mut overs = [0f64; 3];
            for run_idx in 0..runs {
                let trainer = LoggingTrainer::new(
                    FixedRegionTrainer { inner: RecommendedRamenTrainer::new(), fixed: g },
                    run_idx
                );
                let o = bench::run_seeded(UMA, &DECK, &INHERIT, BASE_SEED, run_idx, &trainer)?;
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

    let variant = env::var("VARIANT").context("缺少 VARIANT（空串为 plain 对照）")?;
    let runs: u64 = env::var("RUNS").unwrap_or_else(|_| "20".into()).parse()?;
    let (lowc, lowk) = parse_variant(&variant)?;

    let mut out = File::create("region-matrix-result.csv")?;
    writeln!(
        out,
        "variant,run_idx,a_score,b_score,delta,a_r1,a_r2,a_r3,b_r1,b_r2,b_r3,a_gain3,b_gain3,a_over3,b_over3"
    )?;

    let mut delta_sum = 0f64;
    let mut region_diff = 0usize;
    for run_idx in 0..runs {
        // RecommendedRamenTrainer 含 Mutex 不可 Clone，每局重建（构造开销可忽略）
        let a = bench::run_seeded(
            UMA, &DECK, &INHERIT, BASE_SEED, run_idx, &LoggingTrainer::new(RecommendedRamenTrainer::new(), run_idx)
        )?;
        let b = bench::run_seeded(
            UMA,
            &DECK,
            &INHERIT,
            BASE_SEED,
            run_idx,
            &LoggingTrainer::new(RecommendedRamenTrainer::with_region_overrides(lowc, lowk), run_idx)
        )?;
        let delta = b.score - a.score;
        delta_sum += delta as f64;
        if a.yearly_selected_regions != b.yearly_selected_regions {
            region_diff += 1;
        }
        writeln!(
            out,
            "{variant},{run_idx},{},{},{delta},{},{},{},{},{},{},{},{},{},{}",
            a.score,
            b.score,
            region_cell(&a, 0),
            region_cell(&a, 1),
            region_cell(&a, 2),
            region_cell(&b, 0),
            region_cell(&b, 1),
            region_cell(&b, 2),
            a.yearly_gauge_gain[2],
            b.yearly_gauge_gain[2],
            a.yearly_gauge_overflow[2],
            b.yearly_gauge_overflow[2]
        )?;
    }

    println!(
        "地区变体 {variant}: 平均分差={:.1} ({runs} 局) 地区选择不同局数={region_diff}/{runs}",
        delta_sum / runs as f64
    );
    Ok(())
}