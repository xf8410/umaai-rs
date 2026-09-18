//! 遗传算法参数优化器（针对 [`RecommendedRamenTrainer`]，design.md 定稿方案）。
//!
//! 架构（design.md §1）：GA 不触碰策略代码，一切通过参数覆盖层
//! [`ParamOverride`] 注入；评估跑批 100% 复用 [`crate::bench`]，不另造模拟协议。
//!
//! - **基因组**：77 个基因位（当前仓库实际参数面；设计稿 53 位基于旧快照，
//!   差异见 progress.md），f32 连续编码 ∈ [-1, 1]：`g < 0` = `None`（保留
//!   preset，即现行为），`g >= 0` = `Some(lo + g × (hi − lo))`。全 None 基因组
//!   ≡ 基线策略，是零差异验证的编码保障。
//! - **适应度**（design.md §3.3）：`Σ_build mean(score) − λ_race × Σ mean(!free_race_ok)`；
//!   `yearly_scenario_pt`（剧本 PT）与
//!   `skill_pt`（技能点）分列落盘为诊断列，**不进入适应度**（防牺牲技能点
//!   换 PT 的伪增益）。
//! - **两级评估**（design.md §3.2）：初筛（默认 3 build × 20 局）淘汰明显劣个体，
//!   精评（全部 build × 60 局）评每代前 E 名；holdout（全部 build × 40 局，
//!   `base_seed = 43`）只对每代最优个体复验、不参与选择，防 seed 过拟合。
//! - **算子**（design.md §4）：锦标赛 k=3、均匀交叉率 0.9、每基因变异概率
//!   1/N、高斯变异 σ 从 0.15×区间宽线性衰减至 0.03×区间宽、精英保留 4、
//!   连续 5 代无提升 σ 回升一次。序关系约束用 repair（保序裁剪），不引入罚项。
//! - **可复现性**：评估复用 [`crate::bench::seeded_rngs`]（冻结契约），同一
//!   (基因组, base_seed, 局数) 逐位可复现；适应度按基因组哈希缓存去重。
//!
//! # 零差异验证（design.md §7，GA 准入门槛）
//!
//! 见模块底部单元测试：全 None 基因组配置逐位一致（含决策日志）、
//! 单局锚点 `BASELINE_SCORE = 63870` 原样通过、基因活性冒烟抽样。

use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::OnceLock;
use std::time::Instant;

use anyhow::{Context, Result, ensure};
use rand::{Rng, SeedableRng, rngs::StdRng};
use rand_distr::{Distribution, Normal};
use rayon::prelude::*;

use crate::bench::{self, GameOutcome};
use crate::game::InheritInfo;
use crate::trainer::local_ramen_trainer::ParamOverride;
use crate::card_pool::{CardSelection, SsrPool};
use crate::trainer::{LoggingTrainer, RecommendedRamenTrainer};

/// preset 基线值（repair 与 preset 基因组锚点用）。
///
/// 来源：[`RecommendedRamenTrainer::new()`] 正式 preset（2026-09-14 master）。
/// 仅收录 repair 约束链与基因组锚点涉及的值，其余 preset 值以 `new()` 为准。
pub mod preset_baseline {
    /// 不吃面回合体力硬门限 preset（三年统一）。
    pub const VITAL_REST: i32 = 40;
    /// 吃面回合体力门限 preset（按年；0 = 该年吃面回合不强制休息）。
    pub const VITAL_REST_EATING: [i32; 3] = [40, 40, 0];
    /// 休息目标体力 preset。
    pub const REST_TARGET_VITAL: i32 = 55;
    /// 第三年吃面前软目标体力 preset。
    pub const Y3_PRE_TARGET: i32 = 25;
    /// 友人外出累计上限 preset（按年，累计口径）。
    pub const FRIEND_CAPS: [usize; 3] = [0, 2, 5];
    /// 智力训练体力豁免下限 preset（`i32::MAX` = 不豁免）。
    pub const WISDOM_VITAL_FLOOR_OFF: i32 = i32::MAX;
}

/// 基因值类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GeneKind {
    /// 浮点参数（权重/门限/系数）。
    Float,
    /// 整数参数（解码时四舍五入 + clamp）。
    Int,
    /// 布尔开关（基因 ∈ [0, 0.5) = `false`，[0.5, 1] = `true`）。
    Bool
}

/// 参数所属层（诊断用，与覆盖层两层结构对应）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GeneLayer {
    /// policy 层（`RamenPolicyConfig`）。
    Policy,
    /// trainer 层（`LocalRamenConfig`）。
    Local
}

/// 基因位规格：参数名、类型、GA 取值范围与 preset 锚点。
#[derive(Debug, Clone, Copy)]
pub struct GeneSpec {
    /// 参数名（与覆盖层字段同名，同时用于解码查表与 CSV 基因列名）。
    pub name: &'static str,
    /// 参数层。
    pub layer: GeneLayer,
    /// 值类型。
    pub kind: GeneKind,
    /// GA 取值范围下界（Some 分支映射起点）。
    pub lo: f64,
    /// GA 取值范围上界（Some 分支映射终点）。
    pub hi: f64,
    /// preset 锚点（基因空间 Some 分支的 preset 值）；`None` = preset 状态即
    /// `None`（如智力豁免下限 preset 不豁免）。
    pub preset: Option<f64>,
    /// 分年参数标记：`Some(year)` 表示该基因只作用于第 `year` 年（0-based）。
    pub year: Option<usize>
}

macro_rules! f_gene {
    ($name:expr, $layer:expr, $lo:expr, $hi:expr, $preset:expr) => {
        GeneSpec { name: $name, layer: $layer, kind: GeneKind::Float, lo: $lo, hi: $hi, preset: Some($preset), year: None }
    };
    ($name:expr, $layer:expr, $lo:expr, $hi:expr, $preset:expr, $year:expr) => {
        GeneSpec { name: $name, layer: $layer, kind: GeneKind::Float, lo: $lo, hi: $hi, preset: Some($preset), year: Some($year) }
    };
}
macro_rules! i_gene {
    ($name:expr, $layer:expr, $lo:expr, $hi:expr, $preset:expr) => {
        GeneSpec { name: $name, layer: $layer, kind: GeneKind::Int, lo: $lo, hi: $hi, preset: Some($preset), year: None }
    };
    ($name:expr, $layer:expr, $lo:expr, $hi:expr, $preset:expr, $year:expr) => {
        GeneSpec { name: $name, layer: $layer, kind: GeneKind::Int, lo: $lo, hi: $hi, preset: Some($preset), year: Some($year) }
    };
}
macro_rules! b_gene {
    ($name:expr, $layer:expr, $preset:expr) => {
        GeneSpec { name: $name, layer: $layer, kind: GeneKind::Bool, lo: 0.0, hi: 1.0, preset: Some($preset), year: None }
    };
}

/// 基因组规格表（GA 参数面的唯一权威定义）。
///
/// 范围取值原则：以 preset 为锚、覆盖一个量级左右的邻域；preset 关闭的
/// 参数位（如 `checkpoint_scale`）下界取 0（= 关闭），GA 可探索开启强度。
/// 冻结不进基因组的参数位（`reserve_gain_mode` / `eager_eat` /
/// `ramen_lookahead_weight` / `ramen_lookahead_samples` / `safety_bridge_*` /
/// `friend_rest_max_special` / `deadline_urgency_scale` /
/// `power_overflow_strength` / `power_gap_strength` / `high_fail_penalty`）
/// 保持 preset 原值，理由见 progress.md。
pub const GENE_SPECS: &[GeneSpec] = &[
    // ---- policy 层（RamenPolicyConfig）----
    i_gene!("vital_rest", GeneLayer::Policy, 25.0, 70.0, preset_baseline::VITAL_REST as f64),
    i_gene!("vital_rest_eating_y1", GeneLayer::Policy, 0.0, 70.0, preset_baseline::VITAL_REST_EATING[0] as f64, 0),
    i_gene!("vital_rest_eating_y2", GeneLayer::Policy, 0.0, 70.0, preset_baseline::VITAL_REST_EATING[1] as f64, 1),
    i_gene!("vital_rest_eating_y3", GeneLayer::Policy, 0.0, 70.0, preset_baseline::VITAL_REST_EATING[2] as f64, 2),
    GeneSpec {
        name: "wisdom_vital_floor",
        layer: GeneLayer::Policy,
        kind: GeneKind::Int,
        lo: 20.0,
        hi: 90.0,
        // preset = i32::MAX（不豁免），在基因空间即 None（off）
        preset: None,
        year: None
    },
    i_gene!("motivation_outing", GeneLayer::Policy, 1.0, 6.0, 3.0),
    f_gene!("status_rate", GeneLayer::Policy, 0.5, 2.0, 1.0),
    f_gene!("pt_rate_y1", GeneLayer::Policy, 8.0, 96.0, 16.0, 0),
    f_gene!("pt_rate_y2", GeneLayer::Policy, 8.0, 96.0, 64.0, 1),
    f_gene!("pt_rate_y3", GeneLayer::Policy, 8.0, 96.0, 64.0, 2),
    f_gene!("pt_tradeoff", GeneLayer::Policy, 0.0, 64.0, 16.0),
    GeneSpec {
        name: "pt_tradeoff_shining",
        layer: GeneLayer::Policy,
        kind: GeneKind::Float,
        lo: 16.0,
        hi: 96.0,
        // 真实 preset 随全局标定分段（36/44/52/64），非单值不可作锚点：
        // 基因空间 preset = None（all_preset 基因位保持 None，即保留动态 preset）
        preset: None,
        year: None
    },
    f_gene!("pt_tradeoff_super", GeneLayer::Policy, 0.0, 64.0, 0.0),
    f_gene!("cap_discount_weight", GeneLayer::Policy, 0.0, 1.5, 1.0),
    f_gene!("failure_penalty", GeneLayer::Policy, 10.0, 200.0, 60.0),
    b_gene!("effective_ramen_failure", GeneLayer::Policy, 0.0),
    f_gene!("shining_bonus", GeneLayer::Policy, 10.0, 150.0, 60.0),
    f_gene!("train_vital_value", GeneLayer::Policy, 0.0, 5.0, 1.8),
    f_gene!("rest_base", GeneLayer::Policy, 5.0, 50.0, 20.0),
    f_gene!("rest_vital_value", GeneLayer::Policy, 0.5, 6.0, 2.5),
    i_gene!("rest_target_vital", GeneLayer::Policy, 40.0, 95.0, preset_baseline::REST_TARGET_VITAL as f64),
    f_gene!("race_panel_discount", GeneLayer::Policy, 0.05, 1.0, 0.3),
    f_gene!("race_free_urgency_weight", GeneLayer::Policy, 200.0, 6000.0, 2000.0),
    i_gene!("race_gate_slack", GeneLayer::Policy, 0.0, 3.0, 1.0),
    f_gene!("outing_base", GeneLayer::Policy, 3.0, 40.0, 15.0),
    f_gene!("friend_outing_bonus", GeneLayer::Policy, 10.0, 120.0, 45.0),
    f_gene!("ramen_pt_weight", GeneLayer::Policy, 0.0, 10.0, 2.0),
    f_gene!("ramen_effect_weight", GeneLayer::Policy, 0.0, 10.0, 3.0),
    f_gene!("ramen_special_cost", GeneLayer::Policy, 0.0, 60.0, 12.0),
    f_gene!("ramen_stock_cost", GeneLayer::Policy, 0.0, 2.0, 0.4),
    f_gene!("region_xunlian_weight", GeneLayer::Policy, 5.0, 100.0, 40.0),
    f_gene!("region_hint_weight", GeneLayer::Policy, 2.0, 60.0, 15.0),
    f_gene!("region_youqing_weight", GeneLayer::Policy, 0.0, 10.0, 1.5),
    f_gene!("region_weak_cover_weight", GeneLayer::Policy, 0.0, 60.0, 0.0),
    f_gene!("event_vital_weight", GeneLayer::Policy, 0.0, 6.0, 2.2),
    f_gene!("event_motivation_weight", GeneLayer::Policy, 5.0, 100.0, 40.0),
    f_gene!("event_bad_flag_penalty", GeneLayer::Policy, 50.0, 800.0, 300.0),
    // ---- trainer 层（LocalRamenConfig，三年统一）----
    f_gene!("early_bond_value", GeneLayer::Local, 0.0, 20.0, 8.0),
    f_gene!("hint_bonus", GeneLayer::Local, 0.0, 15.0, 6.0),
    f_gene!("first_friend_click_value", GeneLayer::Local, 20.0, 200.0, 75.0),
    f_gene!("low_friend_bond_value", GeneLayer::Local, 5.0, 100.0, 35.0),
    f_gene!("active_friend_value", GeneLayer::Local, 0.0, 25.0, 8.0),
    i_gene!("feeling_overflow_threshold", GeneLayer::Local, 4.0, 20.0, 8.0),
    f_gene!("overflow_value", GeneLayer::Local, 0.0, 20.0, 8.0),
    f_gene!("max_base_score_sacrifice", GeneLayer::Local, 40.0, 400.0, 140.0),
    f_gene!("status_reserve_max", GeneLayer::Local, 0.0, 120.0, 40.0),
    b_gene!("dynamic_status_balance", GeneLayer::Local, 1.0),
    f_gene!("status_gap_strength", GeneLayer::Local, 0.0, 2.0, 0.5),
    f_gene!("status_overflow_strength", GeneLayer::Local, 0.0, 2.0, 0.5),
    b_gene!("dynamic_vital", GeneLayer::Local, 1.0),
    b_gene!("probabilistic_hint", GeneLayer::Local, 1.0),
    b_gene!("expected_fail", GeneLayer::Local, 1.0),
    f_gene!("checkpoint_scale", GeneLayer::Local, 0.0, 1.5, 0.0),
    f_gene!("rmj_cross_bonus", GeneLayer::Local, 0.0, 300.0, 0.0),
    f_gene!("great_cross_bonus", GeneLayer::Local, 0.0, 300.0, 0.0),
    f_gene!("ramen_window_weight", GeneLayer::Local, 0.0, 0.5, 0.10),
    f_gene!("ramen_train_coupling_weight", GeneLayer::Local, 0.0, 5.0, 2.0),
    f_gene!("ramen_weak_train_boost", GeneLayer::Local, 0.0, 2.0, 0.0),
    f_gene!("friend_hidden_starve_weight", GeneLayer::Local, 0.0, 600.0, 300.0),
    f_gene!("friend_future_hidden_weight", GeneLayer::Local, 0.0, 1.5, 0.0),
    f_gene!("friend_proactive_weight", GeneLayer::Local, 0.0, 400.0, 150.0),
    f_gene!("eat_guarantee_weight", GeneLayer::Local, 0.0, 6.0, 3.0),
    f_gene!("cook2_stock_weight", GeneLayer::Local, 0.0, 120.0, 40.0),
    b_gene!("eat_requires_training", GeneLayer::Local, 1.0),
    b_gene!("eat_requires_covered_train", GeneLayer::Local, 1.0),
    i_gene!("y3_pre_train_vital_target", GeneLayer::Local, 0.0, 60.0, preset_baseline::Y3_PRE_TARGET as f64),
    i_gene!("y3_post_train_vital_target", GeneLayer::Local, 0.0, 50.0, 0.0),
    f_gene!("y3_vital_shortfall_weight", GeneLayer::Local, 0.0, 3.0, 0.5),
    i_gene!("y3_post_train_hard_floor", GeneLayer::Local, 0.0, 50.0, 15.0),
    b_gene!("y3_recovery_horizon", GeneLayer::Local, 1.0),
    b_gene!("friend_outing_replaces_rest", GeneLayer::Local, 1.0),
    i_gene!("friend_outing3_recovery_vital", GeneLayer::Local, 0.0, 80.0, 0.0),
    i_gene!("friend_outing_cumulative_caps_y1", GeneLayer::Local, 0.0, 5.0, preset_baseline::FRIEND_CAPS[0] as f64, 0),
    i_gene!("friend_outing_cumulative_caps_y2", GeneLayer::Local, 0.0, 5.0, preset_baseline::FRIEND_CAPS[1] as f64, 1),
    i_gene!("friend_outing_cumulative_caps_y3", GeneLayer::Local, 0.0, 5.0, preset_baseline::FRIEND_CAPS[2] as f64, 2),
    b_gene!("dynamic_special_targets", GeneLayer::Local, 1.0)
];

/// 参数名 → 基因下标的静态查表（解码用，防手写下标错位）。
pub fn gene_index_map() -> &'static HashMap<&'static str, usize> {
    static MAP: OnceLock<HashMap<&'static str, usize>> = OnceLock::new();
    MAP.get_or_init(|| GENE_SPECS.iter().enumerate().map(|(i, s)| (s.name, i)).collect())
}

/// 按参数名查基因下标。
pub fn gene_index(name: &str) -> Result<usize> {
    gene_index_map()
        .get(name)
        .copied()
        .ok_or_else(|| anyhow::anyhow!("未知基因名: {name}"))
}

/// 基因个数（= [`GENE_SPECS`] 长度）。
pub const GENE_COUNT: usize = GENE_SPECS.len();

/// 基因组：`GENE_COUNT` 个 f32 基因 ∈ [-1, 1]（负 = None 保留 preset，非负 = Some 映射到参数范围）。
#[derive(Debug, Clone, PartialEq)]
pub struct GaGenome(pub Vec<f32>);

impl GaGenome {
    /// 全 None 基因组：与基线策略逐位一致（零差异验证的编码形式）。
    pub fn all_none() -> Self {
        Self(vec![-1.0; GENE_COUNT])
    }

    /// 全 preset 基因组：每个基因锚在 preset 值（preset 即 None 的基因保持 None）。
    pub fn all_preset() -> Self {
        let genes = GENE_SPECS
            .iter()
            .map(|s| match s.preset {
                Some(v) => ((v - s.lo) / (s.hi - s.lo)) as f32,
                None => -1.0
            })
            .collect();
        Self(genes)
    }
}

/// 单基因解码：`g < 0` → None；`g >= 0` → `Some(lo + g × (hi − lo))`（Int 取整 clamp，Bool 按半区）。
fn decode_gene_value(spec: &GeneSpec, g: f32) -> Option<f64> {
    if g < 0.0 {
        return None;
    }
    let g = (g as f64).clamp(0.0, 1.0);
    Some(spec.lo + g * (spec.hi - spec.lo))
}

/// 基因组 → 参数覆盖层（含序关系 repair，保证产出合法个体）。
///
/// 解码语义：`g < 0` = `None`（保留 preset）；`g >= 0` 线性映射到
/// `[lo, hi]`（Int 四舍五入 + clamp，Bool 按 0.5 分界）。
pub fn decode(genome: &GaGenome) -> Result<ParamOverride> {
    ensure!(
        genome.0.len() == GENE_COUNT,
        "基因组长度 {} != 基因个数 {GENE_COUNT}",
        genome.0.len()
    );
    let mut ov = ParamOverride::default();
    // 查表解码 helper：Float / Int / Bool / usize / u32 五种目标类型
    let f = |name: &str, ov: &mut ParamOverride, set: &mut dyn FnMut(&mut ParamOverride, f32)| {
        if let Some(v) = decode_gene_value(&GENE_SPECS[gene_index(name).expect("内建基因名必可查")], genome.0[gene_index(name).expect("内建基因名必可查")]) {
            set(ov, v as f32);
        }
    };
    let i = |name: &str, ov: &mut ParamOverride, set: &mut dyn FnMut(&mut ParamOverride, i32)| {
        let idx = gene_index(name).expect("内建基因名必可查");
        let spec = &GENE_SPECS[idx];
        if let Some(v) = decode_gene_value(spec, genome.0[idx]) {
            set(ov, v.round().clamp(spec.lo, spec.hi) as i64 as i32);
        }
    };
    let b = |name: &str, ov: &mut ParamOverride, set: &mut dyn FnMut(&mut ParamOverride, bool)| {
        let idx = gene_index(name).expect("内建基因名必可查");
        if let Some(v) = decode_gene_value(&GENE_SPECS[idx], genome.0[idx]) {
            // Bool 语义（GeneKind::Bool 文档）：g ∈ [0, 0.5) = false，[0.5, 1] = true。
            // 必须与 all_preset() 锚点回环一致：preset true 锚 1.0 → 解码 true。
            set(ov, v >= 0.5);
        }
    };
    let u = |name: &str, ov: &mut ParamOverride, set: &mut dyn FnMut(&mut ParamOverride, usize)| {
        let idx = gene_index(name).expect("内建基因名必可查");
        let spec = &GENE_SPECS[idx];
        if let Some(v) = decode_gene_value(spec, genome.0[idx]) {
            set(ov, v.round().clamp(spec.lo, spec.hi) as usize);
        }
    };
    let u32v = |name: &str, ov: &mut ParamOverride, set: &mut dyn FnMut(&mut ParamOverride, u32)| {
        let idx = gene_index(name).expect("内建基因名必可查");
        let spec = &GENE_SPECS[idx];
        if let Some(v) = decode_gene_value(spec, genome.0[idx]) {
            set(ov, v.round().clamp(0.0, u32::MAX as f64) as u32);
        }
    };

    // ---- policy 层 ----
    i("vital_rest", &mut ov, &mut |o, v| o.vital_rest = Some(v));
    for y in 0..3 {
        i(&format!("vital_rest_eating_y{}", y + 1), &mut ov, &mut |o, v| {
            o.vital_rest_eating[y] = Some(v)
        });
        f(&format!("pt_rate_y{}", y + 1), &mut ov, &mut |o, v| o.pt_rate[y] = Some(v));
    }
    i("wisdom_vital_floor", &mut ov, &mut |o, v| o.wisdom_vital_floor = Some(v));
    i("motivation_outing", &mut ov, &mut |o, v| o.motivation_outing = Some(v));
    f("status_rate", &mut ov, &mut |o, v| o.status_rate = Some(v));
    f("pt_tradeoff", &mut ov, &mut |o, v| o.pt_tradeoff = Some(v));
    f("pt_tradeoff_shining", &mut ov, &mut |o, v| o.pt_tradeoff_shining = Some(v));
    f("pt_tradeoff_super", &mut ov, &mut |o, v| o.pt_tradeoff_super = Some(v));
    f("cap_discount_weight", &mut ov, &mut |o, v| o.cap_discount_weight = Some(v));
    f("failure_penalty", &mut ov, &mut |o, v| o.failure_penalty = Some(v));
    b("effective_ramen_failure", &mut ov, &mut |o, v| o.effective_ramen_failure = Some(v));
    f("shining_bonus", &mut ov, &mut |o, v| o.shining_bonus = Some(v));
    f("train_vital_value", &mut ov, &mut |o, v| o.train_vital_value = Some(v));
    f("rest_base", &mut ov, &mut |o, v| o.rest_base = Some(v));
    f("rest_vital_value", &mut ov, &mut |o, v| o.rest_vital_value = Some(v));
    i("rest_target_vital", &mut ov, &mut |o, v| o.rest_target_vital = Some(v));
    f("race_panel_discount", &mut ov, &mut |o, v| o.race_panel_discount = Some(v));
    f("race_free_urgency_weight", &mut ov, &mut |o, v| o.race_free_urgency_weight = Some(v));
    u32v("race_gate_slack", &mut ov, &mut |o, v| o.race_gate_slack = Some(v));
    f("outing_base", &mut ov, &mut |o, v| o.outing_base = Some(v));
    f("friend_outing_bonus", &mut ov, &mut |o, v| o.friend_outing_bonus = Some(v));
    f("ramen_pt_weight", &mut ov, &mut |o, v| o.ramen_pt_weight = Some(v));
    f("ramen_effect_weight", &mut ov, &mut |o, v| o.ramen_effect_weight = Some(v));
    f("ramen_special_cost", &mut ov, &mut |o, v| o.ramen_special_cost = Some(v));
    f("ramen_stock_cost", &mut ov, &mut |o, v| o.ramen_stock_cost = Some(v));
    f("region_xunlian_weight", &mut ov, &mut |o, v| o.region_xunlian_weight = Some(v));
    f("region_hint_weight", &mut ov, &mut |o, v| o.region_hint_weight = Some(v));
    f("region_youqing_weight", &mut ov, &mut |o, v| o.region_youqing_weight = Some(v));
    f("region_weak_cover_weight", &mut ov, &mut |o, v| o.region_weak_cover_weight = Some(v));
    f("event_vital_weight", &mut ov, &mut |o, v| o.event_vital_weight = Some(v));
    f("event_motivation_weight", &mut ov, &mut |o, v| o.event_motivation_weight = Some(v));
    f("event_bad_flag_penalty", &mut ov, &mut |o, v| o.event_bad_flag_penalty = Some(v));
    // ---- trainer 层 ----
    f("early_bond_value", &mut ov, &mut |o, v| o.early_bond_value = Some(v));
    f("hint_bonus", &mut ov, &mut |o, v| o.hint_bonus = Some(v));
    f("first_friend_click_value", &mut ov, &mut |o, v| o.first_friend_click_value = Some(v));
    f("low_friend_bond_value", &mut ov, &mut |o, v| o.low_friend_bond_value = Some(v));
    f("active_friend_value", &mut ov, &mut |o, v| o.active_friend_value = Some(v));
    i("feeling_overflow_threshold", &mut ov, &mut |o, v| o.feeling_overflow_threshold = Some(v));
    f("overflow_value", &mut ov, &mut |o, v| o.overflow_value = Some(v));
    f("max_base_score_sacrifice", &mut ov, &mut |o, v| o.max_base_score_sacrifice = Some(v));
    f("status_reserve_max", &mut ov, &mut |o, v| o.status_reserve_max = Some(v));
    b("dynamic_status_balance", &mut ov, &mut |o, v| o.dynamic_status_balance = Some(v));
    f("status_gap_strength", &mut ov, &mut |o, v| o.status_gap_strength = Some(v));
    f("status_overflow_strength", &mut ov, &mut |o, v| o.status_overflow_strength = Some(v));
    b("dynamic_vital", &mut ov, &mut |o, v| o.dynamic_vital = Some(v));
    b("probabilistic_hint", &mut ov, &mut |o, v| o.probabilistic_hint = Some(v));
    b("expected_fail", &mut ov, &mut |o, v| o.expected_fail = Some(v));
    f("checkpoint_scale", &mut ov, &mut |o, v| o.checkpoint_scale = Some(v));
    f("rmj_cross_bonus", &mut ov, &mut |o, v| o.rmj_cross_bonus = Some(v));
    f("great_cross_bonus", &mut ov, &mut |o, v| o.great_cross_bonus = Some(v));
    f("ramen_window_weight", &mut ov, &mut |o, v| o.ramen_window_weight = Some(v));
    f("ramen_train_coupling_weight", &mut ov, &mut |o, v| o.ramen_train_coupling_weight = Some(v));
    f("ramen_weak_train_boost", &mut ov, &mut |o, v| o.ramen_weak_train_boost = Some(v));
    f("friend_hidden_starve_weight", &mut ov, &mut |o, v| o.friend_hidden_starve_weight = Some(v));
    f("friend_future_hidden_weight", &mut ov, &mut |o, v| o.friend_future_hidden_weight = Some(v));
    f("friend_proactive_weight", &mut ov, &mut |o, v| o.friend_proactive_weight = Some(v));
    f("eat_guarantee_weight", &mut ov, &mut |o, v| o.eat_guarantee_weight = Some(v));
    f("cook2_stock_weight", &mut ov, &mut |o, v| o.cook2_stock_weight = Some(v));
    b("eat_requires_training", &mut ov, &mut |o, v| o.eat_requires_training = Some(v));
    b("eat_requires_covered_train", &mut ov, &mut |o, v| o.eat_requires_covered_train = Some(v));
    i("y3_pre_train_vital_target", &mut ov, &mut |o, v| o.y3_pre_train_vital_target = Some(v));
    i("y3_post_train_vital_target", &mut ov, &mut |o, v| o.y3_post_train_vital_target = Some(v));
    f("y3_vital_shortfall_weight", &mut ov, &mut |o, v| o.y3_vital_shortfall_weight = Some(v));
    i("y3_post_train_hard_floor", &mut ov, &mut |o, v| o.y3_post_train_hard_floor = Some(v));
    b("y3_recovery_horizon", &mut ov, &mut |o, v| o.y3_recovery_horizon = Some(v));
    b("friend_outing_replaces_rest", &mut ov, &mut |o, v| o.friend_outing_replaces_rest = Some(v));
    i("friend_outing3_recovery_vital", &mut ov, &mut |o, v| o.friend_outing3_recovery_vital = Some(v));
    for y in 0..3 {
        u(&format!("friend_outing_cumulative_caps_y{}", y + 1), &mut ov, &mut |o, v| {
            o.friend_outing_cumulative_caps[y] = Some(v)
        });
    }
    b("dynamic_special_targets", &mut ov, &mut |o, v| o.dynamic_special_targets = Some(v));
    repair_override(&mut ov);
    Ok(ov)
}

/// 序关系约束 repair（design.md §2，保序裁剪、不引入罚项噪声）。
///
/// 当前代码库的约束链（语义出处见各 config 字段 Rustdoc 与 policy.rs 守门实现）：
/// 1. `rest_target_vital >= vital_rest`（休息目标不低于硬门限）；
/// 2. `vital_rest_eating <= vital_rest`（按年；0 = 该年吃面回合不强制休息，豁免）；
/// 3. `wisdom_vital_floor < vital_rest`（启用豁免时豁免带非空：floor <= vital < rest）；
/// 4. `y3_post_train_vital_target <= y3_pre_train_vital_target`；
/// 5. `y3_post_train_hard_floor <= y3_pre_train_vital_target`；
/// 6. `friend_outing_cumulative_caps` 累计单调且 `<= 5`（与 `matrix_variant`
///    的 friendcap 校验同规则）。
///
/// 裁剪方向：约束链右端"按设计更小/更大"的量向左端值对齐（`None` 基因按
/// preset 参与约束，被裁剪时固化为 `Some`）。
pub fn repair_override(ov: &mut ParamOverride) {
    use preset_baseline as pb;
    let rest = ov.vital_rest.unwrap_or(pb::VITAL_REST);
    // 1. 休息目标 >= 硬门限（目标被抬升到门限）
    let target = ov.rest_target_vital.unwrap_or(pb::REST_TARGET_VITAL);
    if target < rest {
        ov.rest_target_vital = Some(rest);
    }
    // 2. 吃面回合门限 <= 不吃面门限（0 = 关闭，豁免约束）
    for y in 0..3 {
        let eating = ov.vital_rest_eating[y].unwrap_or(pb::VITAL_REST_EATING[y]);
        if eating > 0 && eating > rest {
            ov.vital_rest_eating[y] = Some(rest);
        }
    }
    // 3. 智力豁免带非空：floor <= vital < rest ⇒ floor < rest
    if let Some(floor) = ov.wisdom_vital_floor {
        if floor >= rest {
            ov.wisdom_vital_floor = Some(rest - 1);
        }
    }
    // 4/5. 第三年吃面门禁链：post/hard <= pre
    let pre = ov.y3_pre_train_vital_target.unwrap_or(pb::Y3_PRE_TARGET);
    if let Some(post) = ov.y3_post_train_vital_target {
        if post > pre {
            ov.y3_post_train_vital_target = Some(pre);
        }
    }
    if let Some(hard) = ov.y3_post_train_hard_floor {
        if hard > pre {
            ov.y3_post_train_hard_floor = Some(pre);
        }
    }
    // 6. 友人外出累计上限：单调不减且 <= 5
    let mut caps = [0usize; 3];
    for (y, cap) in caps.iter_mut().enumerate() {
        *cap = ov.friend_outing_cumulative_caps[y]
            .unwrap_or(pb::FRIEND_CAPS[y])
            .min(5);
    }
    caps[2] = caps[2].max(caps[1]).min(5);
    caps[1] = caps[1].max(caps[0]).min(caps[2]);
    caps[0] = caps[0].min(caps[1]);
    for y in 0..3 {
        if ov.friend_outing_cumulative_caps[y].is_some() || caps[y] != pb::FRIEND_CAPS[y] {
            ov.friend_outing_cumulative_caps[y] = Some(caps[y]);
        }
    }
}

/// 基因组哈希（适应度缓存键；SipHash 固定密钥，进程内确定性足够——
/// 缓存生命周期不超过单次 GA 运行）。
pub fn genome_hash(genome: &GaGenome) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    for g in &genome.0 {
        g.to_bits().hash(&mut hasher);
    }
    hasher.finish()
}

/// 布局维度哈希（comp_idx + counts 联合，防不同索引碰巧同 counts 碰撞）。
fn comp_hash(comp_idx: usize, counts: &[usize; 5]) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    comp_idx.hash(&mut hasher);
    counts.hash(&mut hasher);
    hasher.finish()
}

/// 布局 counts → 展示名（如 "3speed+1stamina+1wisdom"）。
fn format_comp_name(counts: &[usize; 5]) -> String {
    let mut parts = Vec::new();
    for (i, &c) in counts.iter().enumerate() {
        if c > 0 {
            parts.push(format!("{c}{}", bench::TYPE_NAMES[i]));
        }
    }
    parts.join("+")
}

/// 评估级别（design.md §3.2 两级评估 + holdout）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EvalLevel {
    /// 初筛：代表 build 子集 × 初筛局数，`base_seed`。
    Screen,
    /// 精评：全部 build × 精评局数，`base_seed`。
    Full,
    /// holdout 复验：全部 build × holdout 局数，`holdout_seed`；不参与选择。
    Holdout
}

impl EvalLevel {
    /// CSV/日志用短名。
    pub fn tag(self) -> &'static str {
        match self {
            EvalLevel::Screen => "s",
            EvalLevel::Full => "f",
            EvalLevel::Holdout => "h"
        }
    }
}

/// 一次评估的适应度与诊断统计（design.md §3.3/§3.4）。
///
/// `fitness` 按 design 公式计算；`mean_skill_pt` 与 `mean_scenario_pt` 为
/// 诊断列（分列落盘，不进适应度——PT 口径纪律）。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ScoreCard {
    /// 评估级别。
    pub level: EvalLevel,
    /// 适应度（Σ_build mean(score) − λ_race × Σ mean(!free_race_ok)）。
    pub fitness: f64,
    /// 全部局数合并的结算评分均值（主指标诊断值）。
    pub mean_score: f64,
    /// 结算评分总体标准差（`bench::summarize` 口径）。
    pub score_std: f64,
    /// 技能点均值（诊断列）。
    pub mean_skill_pt: f64,
    /// 逐年剧本 PT 均值（诊断列，RMJ 清零前归档口径）。
    pub mean_scenario_pt: [f64; 3],
    /// 自选比赛不达标局占比（硬约束诊断）。
    pub race_fail_rate: f64,
    /// RMJ 成功年数均值（0-3）。
    pub mean_rmj_ok: f64,
    /// 单局结算评分最高分（分布诊断，手写逻辑归纳用）。
    pub max_score: f64,
    /// 单局结算评分最低分（分布诊断）。
    pub min_score: f64,
    /// 单局结算评分中位数（分布诊断）。
    pub median_score: f64,
    /// build 数。
    pub n_builds: usize,
    /// 每 build 局数。
    pub runs_per_build: usize
}

impl ScoreCard {
    /// 均值标准误（SE = std / √总局数），用于噪声并列判定。
    pub fn se(&self) -> f64 {
        let n = (self.n_builds * self.runs_per_build).max(1) as f64;
        self.score_std / n.sqrt()
    }
}

/// 评估计数（预算监控）。
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct EvalCounts {
    /// 初筛评估次数（基因组级）。
    pub screen_evals: usize,
    /// 精评评估次数（基因组级）。
    pub full_evals: usize,
    /// holdout 复验次数（基因组级）。
    pub holdout_evals: usize,
    /// 缓存命中次数。
    pub cache_hits: usize
}

/// 明细行：（评估级别，基因组哈希，适应度，单局 CSV 行 = `bench::outcome_to_row` 31 列）。
pub type DetailRow = (EvalLevel, u64, f64, Vec<String>);

/// 适应度评估器抽象（GA 主循环只依赖本 trait；模拟评估与测试 mock 各有一实现）。
pub trait FitnessEvaluator {
    /// 评估指定基因组+布局+配卡选择在指定级别下的适应度（内部按 (组合键, 级别) 缓存去重）。
    /// `key` = genome_hash ⊕ comp_hash ⊕ card_sel_hash（调用方组合）。
    /// `comp_counts` = 该个体的 5 属性普通卡数量分布（合计 5、单类 ≤ 3）。
    fn evaluate(&mut self, key: u64, ov: &ParamOverride, comp_counts: &[usize; 5], card_sel: &CardSelection, level: EvalLevel) -> Result<ScoreCard>;
    /// 取走自上次调用以来积累的单局明细行（GA 逐代落盘后清空，防内存膨胀）。
    fn take_detail_rows(&mut self) -> Vec<DetailRow>;
    /// 累计评估计数。
    fn counts(&self) -> EvalCounts;
    /// 获取 SSR 卡池引用（配卡基因初始化用；mock 实现返回空池）。
    fn pool(&self) -> &SsrPool;
}

/// 模拟评估器：100% 复用 `bench::run_seeded`，rayon 按局并行。
///
/// 布局基因改造后，每个个体自带布局（comp_counts）和配卡（card_sel），
/// 卡组由 comp_counts + CardSelection + pool 动态构建，不再依赖外部 build 列表。
/// 缓存键 = genome_hash ⊕ comp_hash ⊕ card_sel_hash。
pub struct SimFitnessEvaluator {
    /// 友人卡 idrank（随机配卡模式由入口注入；默认 FRIEND_IDRANK）
    friend_idrank: u32,
    uma: u32,
    inherit: InheritInfo,
    /// SSR 卡池（按属性分组，降序排列）。
    pool: SsrPool,
    params: GaParams,
    cache: HashMap<(u64, EvalLevel), ScoreCard>,
    counts: EvalCounts,
    detail_rows: Vec<DetailRow>
}

impl SimFitnessEvaluator {
    /// 从玩家 build 列表构造评估器。
    ///
    /// `screen_build_names`：初筛代表 build 名（design：1 速主 / 1 耐主 / 1 智主，
    /// 由调用方按声明序确定）；卡组用 `DeckComposition::make_deck(&CardPickOpts::default(), friend)`
    /// 与 bench_base 同源同序。
    pub fn new(
        uma: u32,
        friend: u32,
        inherit: InheritInfo,
        params: GaParams,
        pool: SsrPool,
    ) -> Result<Self> {
        Ok(Self {
            uma,
            inherit,
            pool,
            params,
            cache: HashMap::new(),
            counts: EvalCounts::default(),
            detail_rows: Vec::new(),
            friend_idrank: friend,
        })
    }

    /// 运行一个级别下的全部局并聚合（纯计算，不改自身状态）。
    ///
    /// 布局基因改造后：每个个体只有一个布局（comp_counts），构建一副卡组，
    /// 跑 `runs` 局取均值。不再按 build 分组建卡。
    fn run_level(
        &self,
        _key: u64,
        ov: &ParamOverride,
        comp_counts: &[usize; 5],
        card_sel: &CardSelection,
        level: EvalLevel
    ) -> Result<(ScoreCard, Vec<Vec<String>>)> {
        let (runs, base_seed) = match level {
            EvalLevel::Screen => (self.params.screen_runs, self.params.base_seed),
            EvalLevel::Full => (self.params.full_runs, self.params.base_seed),
            EvalLevel::Holdout => (self.params.holdout_runs, self.params.holdout_seed)
        };
        let runs = runs.max(1);

        // 按 comp_counts + card_sel 动态构建卡组；友人卡 idrank 由入口注入
        // （随机配卡模式下每 run 不同；同一 run 内恒定，缓存键不含友人无碰撞）
        let deck = {
            let mut cs = card_sel.clone();
            cs.friend_idrank = self.friend_idrank;
            cs.build_deck(&self.pool, comp_counts)?
        };
        let build_name = format_comp_name(comp_counts);

        let jobs: Vec<usize> = (0..runs).collect();
        let outcomes: Vec<GameOutcome> = jobs
            .into_par_iter()
            .map(|r| {
                let trainer = LoggingTrainer::new(
                    RecommendedRamenTrainer::with_overrides_for_rollout(ov),
                    0
                );
                bench::run_seeded(
                    self.uma,
                    &deck,
                    &self.inherit,
                    base_seed,
                    r as u64,
                    &trainer
                )
                .with_context(|| format!("run_seeded 失败: layout={build_name} run={r} level={level:?}"))
            })
            .collect::<Result<Vec<_>>>()?;

        let mut rows = Vec::with_capacity(outcomes.len());
        let mut scores = Vec::with_capacity(outcomes.len());
        let mut skill_pts = Vec::with_capacity(outcomes.len());
        let mut scenario_pt = [0.0f64; 3];
        let mut race_fails = 0.0f64;
        let mut rmj_ok_sum = 0.0f64;
        let deck_str = deck.iter().map(|d| d.to_string()).collect::<Vec<_>>().join("+");
        for outcome in &outcomes {
            let mut row = bench::outcome_to_row(&build_name, outcome);
            row.push(deck_str.clone());
            rows.push(row);
            scores.push(outcome.score as f64);
            skill_pts.push(outcome.skill_pt as f64);
            for (y, v) in scenario_pt.iter_mut().enumerate() {
                *v += outcome.yearly_scenario_pt[y] as f64;
            }
            if !outcome.free_race_ok {
                race_fails += 1.0;
            }
            rmj_ok_sum += outcome.rmj_ok as f64;
        }
        let stats = bench::summarize(&scores);
        let n = scores.len() as f64;
        // fitness = mean(score) − λ_race × race_fail_rate（单布局）
        let race_fail_rate = race_fails / n;
        let fitness = stats.mean - self.params.lambda_race * race_fail_rate;
        for v in scenario_pt.iter_mut() {
            *v /= n;
        }
        let mut sorted_scores = scores.clone();
        sorted_scores.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let median_score = if sorted_scores.len() % 2 == 1 {
            sorted_scores[sorted_scores.len() / 2]
        } else {
            (sorted_scores[sorted_scores.len() / 2 - 1] + sorted_scores[sorted_scores.len() / 2]) / 2.0
        };
        let card = ScoreCard {
            level,
            fitness,
            mean_score: stats.mean,
            score_std: stats.std,
            mean_skill_pt: skill_pts.iter().sum::<f64>() / n,
            mean_scenario_pt: scenario_pt,
            race_fail_rate,
            mean_rmj_ok: rmj_ok_sum / n,
            max_score: scores.iter().cloned().fold(f64::NEG_INFINITY, f64::max),
            min_score: scores.iter().cloned().fold(f64::INFINITY, f64::min),
            median_score,
            n_builds: 1,
            runs_per_build: runs
        };
        Ok((card, rows))
    }
}

impl FitnessEvaluator for SimFitnessEvaluator {
    fn evaluate(&mut self, key: u64, ov: &ParamOverride, comp_counts: &[usize; 5], card_sel: &CardSelection, level: EvalLevel) -> Result<ScoreCard> {
        if let Some(card) = self.cache.get(&(key, level)) {
            self.counts.cache_hits += 1;
            return Ok(*card);
        }
        let (card, rows) = self.run_level(key, ov, comp_counts, card_sel, level)?;
        self.detail_rows.extend(rows.into_iter().map(|r| (level, key, card.fitness, r)));
        match level {
            EvalLevel::Screen => self.counts.screen_evals += 1,
            EvalLevel::Full => self.counts.full_evals += 1,
            EvalLevel::Holdout => self.counts.holdout_evals += 1
        }
        self.cache.insert((key, level), card);
        Ok(card)
    }

    fn take_detail_rows(&mut self) -> Vec<DetailRow> {
        std::mem::take(&mut self.detail_rows)
    }

    fn counts(&self) -> EvalCounts {
        self.counts
    }

    fn pool(&self) -> &SsrPool {
        &self.pool
    }
}

/// GA 代际参数与评估协议参数（默认值 = design.md §3/§4 定稿值）。
#[derive(Debug, Clone)]
pub struct GaParams {
    /// 种群规模（design：60）。
    pub pop: usize,
    /// 代数（design：40）。
    pub gens: usize,
    /// 精英保留数（design：4）。
    pub elitism: usize,
    /// 锦标赛选择 k（design：3）。
    pub tournament_k: usize,
    /// 均匀交叉率（design：0.9）。
    pub crossover_rate: f64,
    /// 高斯变异 σ 初值（×区间宽，design：0.15）。
    pub sigma0: f64,
    /// 高斯变异 σ 下界（×区间宽，design：0.03）。
    pub sigma_min: f64,
    /// 早期停滞判定代数（连续 N 代最优无提升 → σ 回升一次，design：5）。
    pub stagnation_gens: usize,
    /// 初筛 build 数（design：3）。
    pub screen_builds: usize,
    /// 初筛局数/build（design：20，与 bench_config 默认 runs 对齐）。
    pub screen_runs: usize,
    /// 精评 build 数（design：全部 7）。
    pub full_builds: usize,
    /// 精评局数/build（design：60）。
    pub full_runs: usize,
    /// holdout 局数/build（design：40）。
    pub holdout_runs: usize,
    /// 评估基础种子（design：固定 42，与 bench 基线一致，GA 期间不得更换）。
    pub base_seed: u64,
    /// holdout 种子（design：43，防过拟合）。
    pub holdout_seed: u64,
    /// 自选比赛不达标惩罚系数（design 占位 800/局，实现期以基线跑批校准）。
    pub lambda_race: f64,
    /// GA 自身随机种子（与评估种子分离）。
    pub ga_seed: u64,
    /// 初始化时基因取 None 的比例（design 未定，取 0.3 保证 None 语义可被搜索）。
    pub none_gene_ratio: f64
}

impl Default for GaParams {
    fn default() -> Self {
        Self {
            pop: 60,
            gens: 40,
            elitism: 4,
            tournament_k: 3,
            crossover_rate: 0.9,
            sigma0: 0.15,
            sigma_min: 0.03,
            stagnation_gens: 5,
            screen_builds: 3,
            screen_runs: 20,
            full_builds: 7,
            full_runs: 60,
            holdout_runs: 40,
            base_seed: 42,
            holdout_seed: 43,
            lambda_race: 800.0,
            ga_seed: 42,
            none_gene_ratio: 0.3
        }
    }
}

/// 单个个体：基因组 + 布局 + 配卡选择 + 解码后的覆盖层 + 两级评估缓存。
///
/// 三维搜索空间：77 参数基因 × 101 布局 × 配卡选择。
#[derive(Debug, Clone)]
struct Individual {
    genome: GaGenome,
    /// 布局索引（0..101），对应 `bench::all_compositions()` 中的一行。
    comp_idx: usize,
    /// 布局 counts 缓存（避免每次查表）。
    comp_counts: [usize; 5],
    card_sel: CardSelection,
    /// 组合键 = genome_hash ⊕ comp_hash ⊕ card_sel.hash_key()（适应度缓存键）。
    key: u64,
    ov: ParamOverride,
    screen: Option<ScoreCard>,
    full: Option<ScoreCard>
}

impl Individual {
    /// 当前参与选择的适应度（优先精评值）。
    fn display_fitness(&self) -> f64 {
        self.full
            .as_ref()
            .or(self.screen.as_ref())
            .map(|c| c.fitness)
            .unwrap_or(f64::NEG_INFINITY)
    }

    /// 当前适应度对应的标准误（噪声并列判定用）。
    fn display_se(&self) -> f64 {
        self.full
            .as_ref()
            .or(self.screen.as_ref())
            .map(|c| c.se())
            .unwrap_or(f64::INFINITY)
    }
}

/// 单代运行摘要（CSV 落盘 + 终端打印）。
#[derive(Debug, Clone)]
pub struct GaGenRecord {
    /// 代号（0-based）。
    pub generation: usize,
    /// 本代最优适应度（优先精评值）。
    pub best_fitness: f64,
    /// 本代最优个体是否经过精评。
    pub best_is_full: bool,
    /// 种群适应度均值。
    pub mean_fitness: f64,
    /// 本代精评个体数。
    pub full_count: usize,
    /// 本代缓存命中数。
    pub cache_hits: usize,
    /// 本代 σ。
    pub sigma: f64,
    /// 停滞计数（连续无提升代数）。
    pub stagnation_counter: usize,
    /// 本代冠军 holdout 适应度（None = 未评估）。
    pub holdout_fitness: Option<f64>,
    /// 本代冠军 holdout 结算评分均值。
    pub holdout_mean_score: Option<f64>,
    /// 本代耗时（毫秒）。
    pub elapsed_ms: f64
}

/// GA 运行报告。
#[derive(Debug, Clone)]
pub struct GaReport {
    /// 全程最优基因组（精评口径）。
    pub best_genome: GaGenome,
    /// 全程最优布局索引。
    pub best_comp_idx: usize,
    /// 全程最优配卡选择。
    pub best_card_sel: CardSelection,
    /// 全程最优覆盖层（解码 + repair 后）。
    pub best_override: ParamOverride,
    /// 全程最优适应度（精评口径）。
    pub best_fitness: f64,
    /// 全程最优个体的精评卡片（诊断列齐全）。
    pub best_card: ScoreCard,
    /// 逐代摘要。
    pub history: Vec<GaGenRecord>,
    /// 全程 holdout 复验卡片（末代冠军）。
    pub holdout_card: Option<ScoreCard>,
    /// 过拟合哨兵：最优适应度（eval seed）− holdout 适应度（holdout seed）。
    /// 为正且持续增大即提示 seed 过拟合（design §3.2，不参与选择）。
    pub overfit_gap: Option<f64>,
    /// σ 受控重启次数。
    pub stagnation_resets: usize,
    /// 评估计数。
    pub counts: EvalCounts,
    /// 总耗时（毫秒）。
    pub elapsed_ms: f64
}

/// GA 主循环（design.md §4 算子与代际参数）。
#[derive(Debug, Clone)]
pub struct GaOptimizer {
    /// 代际参数。
    pub params: GaParams
}

impl GaOptimizer {
    /// 用指定参数构造优化器。
    pub fn new(params: GaParams) -> Self {
        Self { params }
    }

    /// 初始种群：个体 0 = 全 None（基线冠军）、个体 1 = 全 preset、其余随机
    /// （每基因以 `none_gene_ratio` 概率取 None，否则 Some 区间均匀）。
    fn init_population(&self, rng: &mut StdRng, pool: &SsrPool) -> Vec<Individual> {
        let mut pop = Vec::with_capacity(self.params.pop);
        for idx in 0..self.params.pop {
            let genome = match idx {
                0 => GaGenome::all_none(),
                1 => GaGenome::all_preset(),
                _ => {
                    let genes = (0..GENE_COUNT)
                        .map(|_| {
                            if rng.random::<f64>() < self.params.none_gene_ratio {
                                rng.random_range(-1.0..=-0.05f32)
                            } else {
                                rng.random_range(0.0..=1.0f32)
                            }
                        })
                        .collect();
                    GaGenome(genes)
                }
            };
            // 布局基因：个体 0/1 用默认布局，其余随机
            let comp_idx = if idx < 2 {
                bench::DEFAULT_COMP_INDEX
            } else {
                rng.random_range(0..bench::COMPOSITION_COUNT)
            };
            // 配卡选择：个体 0/1 用默认（池内首选），其余随机扰动一个属性
            let mut card_sel = CardSelection::default_top(pool);
            if idx >= 2 {
                card_sel.mutate_one(pool, rng);
            }
            pop.push(Self::make_individual(genome, comp_idx, card_sel, pool));
        }
        pop
    }

    /// 基因组 + 布局 + 配卡选择 → 个体（解码 + repair + 组合哈希）。
    ///
    /// **配卡 clamp**：`indices[attr]` 必须在 `[0, pool_size - count]` 范围内，
    /// 否则 `build_deck` 中 `indices[attr] + j` 会越界。此处统一修复，
    /// 上游 mutate/crossover 无需感知布局-配卡的耦合约束。
    fn make_individual(genome: GaGenome, comp_idx: usize, mut card_sel: CardSelection, pool: &SsrPool) -> Individual {
        let gh = genome_hash(&genome);
        let comp_counts = bench::all_compositions()[comp_idx];
        // Clamp: 确保 indices[attr] + counts[attr] - 1 < pool_size
        for attr in 0..crate::card_pool::ATTR_COUNT {
            let max_start = pool.pool_size(attr).saturating_sub(comp_counts[attr]);
            if card_sel.indices[attr] > max_start {
                card_sel.indices[attr] = max_start;
            }
        }
        let ch = comp_hash(comp_idx, &comp_counts);
        let sh = card_sel.hash_key();
        let key = gh ^ ch.wrapping_mul(0x9e3779b97f4a7c15) ^ sh.wrapping_mul(0x517cc1b727220a95);
        let ov = decode(&genome).expect("解码含 repair，合法基因组必成功");
        Individual {
            genome,
            comp_idx,
            comp_counts,
            card_sel,
            key,
            ov,
            screen: None,
            full: None
        }
    }

    /// 锦标赛选择（k 无放回抽样，适应度最大者胜出；并列取下标小者）。
    fn tournament<'a>(&self, pop: &'a [Individual], rng: &mut StdRng) -> &'a Individual {
        let k = self.params.tournament_k.clamp(1, pop.len());
        let mut picked: Vec<usize> = (0..pop.len()).collect();
        // 部分 Fisher–Yates：抽样 k 个不重复下标
        for i in 0..k {
            let j = rng.random_range(i..picked.len());
            picked.swap(i, j);
        }
        let mut best = &pop[picked[0]];
        let mut best_idx = picked[0];
        for &idx in &picked[1..k] {
            let cand = &pop[idx];
            let better = cand.display_fitness() > best.display_fitness()
                || (cand.display_fitness() == best.display_fitness() && idx < best_idx);
            if better {
                best = cand;
                best_idx = idx;
            }
        }
        best
    }

    /// 均匀交叉（design：率 0.9；未交叉时克隆父本 A）。
    fn crossover(&self, a: &GaGenome, b: &GaGenome, rng: &mut StdRng) -> GaGenome {
        if rng.random::<f64>() >= self.params.crossover_rate {
            return a.clone();
        }
        GaGenome(
            a.0.iter()
                .zip(b.0.iter())
                .map(|(x, y)| if rng.random::<bool>() { *y } else { *x })
                .collect()
        )
    }

    /// 高斯变异（design：每基因概率 1/N；σ 按代线性衰减；布尔翻转；
    /// None 基因变异时以 preset 锚点 + 噪声激活）。
    fn mutate(&self, genome: &mut GaGenome, sigma: f64, rng: &mut StdRng) {
        let pm = 1.0 / GENE_COUNT as f64;
        let normal = Normal::new(0.0, sigma.max(1e-6)).expect("σ > 0");
        for (i, g) in genome.0.iter_mut().enumerate() {
            if rng.random::<f64>() >= pm {
                continue;
            }
            let spec = &GENE_SPECS[i];
            if spec.kind == GeneKind::Bool {
                // 布尔翻转：None = 翻到 preset 反向（g=1.0 译为 false、g=0.0 译为 true）；
                // Some = 取反（g<0.5 即 true → 1.0 即 false，反之亦然）
                *g = if *g < 0.0 {
                    match spec.preset {
                        Some(p) if p >= 0.5 => 1.0,
                        _ => 0.0
                    }
                } else if *g < 0.5 {
                    1.0
                } else {
                    0.0
                };
            } else if *g < 0.0 {
                // None → 以 preset 锚点 + 噪声激活（preset 即 None 的基因取区间中点）
                let anchor = spec.preset.unwrap_or((spec.lo + spec.hi) / 2.0);
                let g_star = ((anchor - spec.lo) / (spec.hi - spec.lo)) as f32;
                let delta = normal.sample(rng) as f32;
                *g = (g_star + delta).clamp(0.0, 1.0);
            } else {
                let delta = normal.sample(rng) as f32;
                *g = (*g + delta).clamp(-1.0, 1.0);
            }
        }
    }

    /// 布局交叉：按概率从父本 A 或 B 继承 comp_idx。
    fn crossover_comp(a_comp: usize, b_comp: usize, rng: &mut StdRng) -> usize {
        if rng.random::<bool>() { b_comp } else { a_comp }
    }

    /// 布局变异：以概率 1/COMPOSITION_COUNT 随机换一个合法布局。
    fn mutate_comp(comp_idx: usize, rng: &mut StdRng) -> usize {
        if rng.random::<f64>() < 1.0 / bench::COMPOSITION_COUNT as f64 {
            loop {
                let new_idx = rng.random_range(0..bench::COMPOSITION_COUNT);
                if new_idx != comp_idx {
                    return new_idx;
                }
            }
        }
        comp_idx
    }

    /// 当代变异 σ：线性衰减（0.15 → 0.03 ×区间宽），停滞触发的代回升到 σ0。
    fn sigma_for_gen(&self, generation_idx: usize, stagnation_reset: bool) -> f64 {
        if stagnation_reset {
            return self.params.sigma0;
        }
        if self.params.gens <= 1 {
            return self.params.sigma0;
        }
        let t = generation_idx as f64 / (self.params.gens - 1) as f64;
        self.params.sigma0 - (self.params.sigma0 - self.params.sigma_min) * t
    }

    /// 运行完整 GA（选择/交叉/变异 + 两级评估 + holdout 复验 + 逐代摘要）。
    ///
    /// 流程（design.md §3/§4）：
    /// 1. 初始种群：个体 0 = 全 None（基线冠军）、个体 1 = 全 preset、其余随机；
    /// 2. 每代：无精评值的个体过初筛 → 按当前适应度（SE 容差噪声并列随机保序）
    ///    取前 E 名补精评 → 按精评优先适应度排精英 → 冠军跑 holdout（缓存去重）；
    /// 3. 停滞计数：全程最优精评值连续 `stagnation_gens` 代无提升 → 下代 σ 回升；
    /// 4. 繁殖：精英直接保留，其余锦标赛选父 + 均匀交叉 + 高斯变异；
    /// 5. 收尾：全局最优（精评口径）做最终 holdout，报告过拟合哨兵。
    pub fn run(&self, evaluator: &mut dyn FitnessEvaluator) -> Result<GaReport> {
        let start = Instant::now();
        let mut rng = StdRng::seed_from_u64(self.params.ga_seed);
        let mut pop = self.init_population(&mut rng, evaluator.pool());
        let mut history = Vec::with_capacity(self.params.gens);
        // 全程最优（精评口径）：(fitness, genome, comp_idx, card_sel, override, card)
        let mut best: Option<(f64, GaGenome, usize, CardSelection, ParamOverride, ScoreCard)> = None;
        let mut stagnation_counter = 0usize;
        let mut stagnation_resets = 0usize;

        for generation_idx in 0..self.params.gens {
            let gen_start = Instant::now();
            // σ：停滞触发的代回升到 σ0，否则按代线性衰减（design §4）
            let stagnated = stagnation_counter >= self.params.stagnation_gens;
            let sigma = self.sigma_for_gen(generation_idx, stagnated);
            if stagnated {
                stagnation_counter = 0;
                stagnation_resets += 1;
            }
            let cache_hits_before = evaluator.counts().cache_hits;
            // 代初快照：用于停滞判定（全程最优精评值是否在本代被刷新）
            let prev_best_fitness = best.as_ref().map(|b| b.0);

            // 1) 初筛：尚无任何评估值的个体全部过初筛
            for ind in pop.iter_mut() {
                if ind.full.is_none() && ind.screen.is_none() {
                    ind.screen = Some(evaluator.evaluate(ind.key, &ind.ov, &ind.comp_counts, &ind.card_sel, EvalLevel::Screen)?);
                }
            }

            // 2) 精评晋升：按当前适应度取前 E 名补精评（噪声并列随机保序）
            //    注：旧版"无差异区间+随机tiebreak"比较器违反全序公理（传递性），
            //    Rust sort 检测后 panic。改为预计算排序键 (fitness desc, tiebreak desc, index asc)。
            let promotion_slots = self.params.elitism.max(1).min(pop.len());
            let tiebreak: Vec<f64> = pop.iter().map(|_| rng.random::<f64>()).collect();
            let mut order: Vec<usize> = (0..pop.len()).collect();
            order.sort_by(|&a, &b| {
                let fa = pop[a].display_fitness();
                let fb = pop[b].display_fitness();
                fb.total_cmp(&fa)
                    .then(tiebreak[b].total_cmp(&tiebreak[a]))
                    .then(a.cmp(&b))
            });
            for &i in order.iter().take(promotion_slots) {
                if pop[i].full.is_none() {
                    pop[i].full = Some(evaluator.evaluate(pop[i].key, &pop[i].ov, &pop[i].comp_counts, &pop[i].card_sel, EvalLevel::Full)?);
                }
            }

            // 3) 排序刷新：按精评优先适应度（确定性 tie-break：哈希小者优先）
            order.sort_by(|&a, &b| {
                pop[b]
                    .display_fitness()
                    .total_cmp(&pop[a].display_fitness())
                    .then(pop[a].key.cmp(&pop[b].key))
            });
            let best_ind = &pop[order[0]];
            let best_fitness = best_ind.display_fitness();
            let best_is_full = best_ind.full.is_some();
            let mean_fitness = pop.iter().map(|i| i.display_fitness()).sum::<f64>() / pop.len() as f64;
            let full_count = pop.iter().filter(|i| i.full.is_some()).count();

            // 全程最优跟踪（仅精评口径入榜，跨代可比）
            if let Some(card) = best_ind.full.as_ref() {
                let is_better = match &best {
                    Some((bf, _, _, _, _, _)) => card.fitness > *bf,
                    None => true
                };
                if is_better {
                    best = Some((card.fitness, best_ind.genome.clone(), best_ind.comp_idx, best_ind.card_sel, best_ind.ov.clone(), *card));
                }
            }

            // 4) holdout 复验：本代冠军、不参与选择（缓存按 (key, level) 去重）
            let gen_holdout = evaluator.evaluate(best_ind.key, &best_ind.ov, &best_ind.comp_counts, &best_ind.card_sel, EvalLevel::Holdout)?;

            // 5) 停滞计数：全程最优精评值严格高于代初快照才算提升
            let improved = match (best.as_ref().map(|b| b.0), prev_best_fitness) {
                (Some(cur), Some(prev)) => cur > prev + 1e-9,
                (Some(_), None) => true, // 首个精评个体入榜
                (None, _) => false
            };
            if improved {
                stagnation_counter = 0;
            } else {
                stagnation_counter += 1;
            }

            history.push(GaGenRecord {
                generation: generation_idx,
                best_fitness,
                best_is_full,
                mean_fitness,
                full_count,
                cache_hits: evaluator.counts().cache_hits - cache_hits_before,
                sigma,
                stagnation_counter,
                holdout_fitness: Some(gen_holdout.fitness),
                holdout_mean_score: Some(gen_holdout.mean_score),
                elapsed_ms: gen_start.elapsed().as_secs_f64() * 1000.0
            });

            // 6) 繁殖下一代：精英直接保留，其余锦标赛 + 均匀交叉 + 高斯变异
            if generation_idx + 1 < self.params.gens {
                let mut next: Vec<Individual> = Vec::with_capacity(self.params.pop);
                for &i in order.iter().take(self.params.elitism.min(pop.len())) {
                    next.push(pop[i].clone());
                }
                while next.len() < self.params.pop {
                    let a = self.tournament(&pop, &mut rng);
                    let b = self.tournament(&pop, &mut rng);
                    let mut child_genome = self.crossover(&a.genome, &b.genome, &mut rng);
                    self.mutate(&mut child_genome, sigma, &mut rng);
                    // 布局交叉 + 变异
                    let mut child_comp = Self::crossover_comp(a.comp_idx, b.comp_idx, &mut rng);
                    child_comp = Self::mutate_comp(child_comp, &mut rng);
                    // 配卡交叉 + 变异
                    let mut child_card = CardSelection::crossover(&a.card_sel, &b.card_sel, &mut rng);
                    if rng.random::<f64>() < 1.0 / crate::card_pool::ATTR_COUNT as f64 {
                        child_card.mutate_one(evaluator.pool(), &mut rng);
                    }
                    next.push(Self::make_individual(child_genome, child_comp, child_card, evaluator.pool()));
                }
                pop = next;
            }
        }

        // 收尾：全局最优的最终 holdout 复验（此前每代冠军已跑过，通常缓存命中）
        let (best_fitness, best_genome, best_comp_idx, best_card_sel, best_override, best_card) = best
            .ok_or_else(|| anyhow::anyhow!("GA 未产生任何精评个体"))?;
        let best_comp_counts = bench::all_compositions()[best_comp_idx];
        let gh = genome_hash(&best_genome);
        let ch = comp_hash(best_comp_idx, &best_comp_counts);
        let sh = best_card_sel.hash_key();
        let final_key = gh ^ ch.wrapping_mul(0x9e3779b97f4a7c15) ^ sh.wrapping_mul(0x517cc1b727220a95);
        let final_holdout = evaluator.evaluate(
            final_key,
            &best_override,
            &best_comp_counts,
            &best_card_sel,
            EvalLevel::Holdout
        )?;
        let holdout_card = Some(final_holdout);
        let overfit_gap = Some(best_fitness - final_holdout.fitness);

        Ok(GaReport {
            best_genome,
            best_comp_idx,
            best_card_sel,
            best_override,
            best_fitness,
            best_card,
            history,
            holdout_card,
            overfit_gap,
            stagnation_resets,
            counts: evaluator.counts(),
            elapsed_ms: start.elapsed().as_secs_f64() * 1000.0
        })
    }
}

/// 初筛代表 build 挑选（design §3.2：1 速主、1 耐主、1 智主，按声明序取风格差异最大者）。
///
/// 实现：按 [速, 耐, 智] 类型依次在"未被选走"的 build 中取该类型数量最大者
/// （并列取声明序靠前者）；不足时按声明序补齐。
pub fn select_screen_builds(builds: &[bench::DeckComposition], count: usize) -> Vec<String> {
    let mut chosen: Vec<usize> = Vec::new();
    for card_type in [0usize, 1, 4] {
        if chosen.len() >= count {
            break;
        }
        let best = builds
            .iter()
            .enumerate()
            .filter(|(i, _)| !chosen.contains(i))
            .max_by_key(|(i, b)| (b.counts[card_type], std::cmp::Reverse(*i)));
        if let Some((i, _)) = best {
            chosen.push(i);
        }
    }
    for (i, _) in builds.iter().enumerate() {
        if chosen.len() >= count {
            break;
        }
        if !chosen.contains(&i) {
            chosen.push(i);
        }
    }
    chosen.sort();
    chosen.into_iter().map(|i| builds[i].name.clone()).collect()
}

// ==================== 单元测试 ====================

#[cfg(test)]
mod tests {
    use anyhow::Result;
    use rand::{SeedableRng, rngs::StdRng};

    use super::*;
    use crate::gamedata::init_global;
    use crate::trainer::LoggingTrainer;
    use crate::utils::{Checks, get_workspace_root, init_test_logger};

    /// 测试用锚点常量（与 bench.rs 测试模块同源；那边是模块私有，这里复制一份）。
    const UMA: u32 = 102601;
    const DECK: [u32; 6] = [302424, 302894, 303044, 302924, 303024, 303054];
    const INHERIT: InheritInfo = InheritInfo {
        blue_count: [15, 3, 0, 0, 0],
        extra_count: [0, 30, 0, 0, 30, 30]
    };
    const FRIEND: u32 = 303054;
    // 基线锚点（seed=42/run0/TEST_DECK/TEST_INHERIT，与 bench.rs 测试同源口径）。
    // 2026-09-17 重录（fork master）：上游合并 875dd2c（9 旋钮 preset 定稿 + 评估
    // 核心/trainer/policy 行为变化）后实测 64752→92393。GA 的零差异门是
    // "覆盖层 ≡ 现行为"的路径等价（通道 A/B 对比），本锚点只用于锁定当前基线可复现。
    const MASTER_ANCHOR_SCORE: i32 = 92393;
    const MASTER_ANCHOR_FIVE: [i32; 5] = [3337, 2445, 2107, 1230, 1251];

    /// 测试引导：工作目录 + 日志 + 全局数据（与 bench.rs 测试同款）。
    fn bootstrap() -> Result<()> {
        let workspace_root = get_workspace_root()?;
        std::env::set_current_dir(&workspace_root)?;
        let _ = init_test_logger("error");
        let _ = init_global();
        Ok(())
    }

    /// 确定性 mock 评估器：fitness 由基因组哈希派生，不跑模拟。
    /// 用于验证 GA 主循环的选择/交叉/变异/缓存/晋升逻辑本身。
    struct MockEvaluator {
        cache: HashMap<(u64, EvalLevel), ScoreCard>,
        counts: EvalCounts,
        detail_rows: Vec<DetailRow>,
        /// 实际执行计算的 (key, level) 序列（验证缓存去重）。
        compute_log: Vec<(u64, EvalLevel)>
    }

    impl MockEvaluator {
        fn new() -> Self {
            Self {
                cache: HashMap::new(),
                counts: EvalCounts::default(),
                detail_rows: Vec::new(),
                compute_log: Vec::new()
            }
        }

        /// 确定性 mock 卡片：fitness ∈ [0, 10000)，随 key 变化。
        fn mock_card(level: EvalLevel, key: u64) -> ScoreCard {
            let fitness = ((key >> 11) % 10_000) as f64;
            ScoreCard {
                level,
                fitness,
                mean_score: fitness,
                score_std: 100.0,
                mean_skill_pt: 0.0,
                mean_scenario_pt: [0.0; 3],
                race_fail_rate: 0.0,
                mean_rmj_ok: 3.0,
                max_score: fitness,
                min_score: fitness,
                median_score: fitness,
                n_builds: 3,
                runs_per_build: 20
            }
        }
    }

    impl FitnessEvaluator for MockEvaluator {
        fn evaluate(&mut self, key: u64, _ov: &ParamOverride, _comp_counts: &[usize; 5], _card_sel: &CardSelection, level: EvalLevel) -> Result<ScoreCard> {
            if let Some(card) = self.cache.get(&(key, level)) {
                self.counts.cache_hits += 1;
                return Ok(*card);
            }
            self.compute_log.push((key, level));
            match level {
                EvalLevel::Screen => self.counts.screen_evals += 1,
                EvalLevel::Full => self.counts.full_evals += 1,
                EvalLevel::Holdout => self.counts.holdout_evals += 1
            }
            let card = Self::mock_card(level, key);
            self.cache.insert((key, level), card);
            self.detail_rows
                .push((level, key, card.fitness, vec!["mock".to_string()]));
            Ok(card)
        }

        fn take_detail_rows(&mut self) -> Vec<DetailRow> {
            std::mem::take(&mut self.detail_rows)
        }

        fn counts(&self) -> EvalCounts {
            self.counts
        }

        fn pool(&self) -> &SsrPool {
            // mock 用空池（测试不涉及真实卡组构建）
            static EMPTY: std::sync::OnceLock<SsrPool> = std::sync::OnceLock::new();
            EMPTY.get_or_init(|| SsrPool {
                pools: std::array::from_fn(|_| Vec::new()),
                excluded: Vec::new(),
                exclude_chara_ids: Vec::new(),
            })
        }
    }

    /// 规格表自检：名字唯一、范围合法、preset 锚点落在范围内、gene_index 回环。
    #[test]
    fn ga_spec_sanity() -> Result<()> {
        let mut c = Checks::new();
        println!("基因总数 = {GENE_COUNT}（specs = {}）", GENE_SPECS.len());
        c.check(GENE_SPECS.len() == GENE_COUNT, "GENE_COUNT 与规格表长度一致");

        for (i, spec) in GENE_SPECS.iter().enumerate() {
            c.check(gene_index(spec.name)? == i, &format!("gene_index 回环: {}", spec.name));
            c.check(spec.lo < spec.hi, &format!("{} 范围 lo < hi", spec.name));
            match spec.kind {
                GeneKind::Bool => {
                    c.check(spec.lo == 0.0 && spec.hi == 1.0, &format!("{} Bool 范围固定 [0,1]", spec.name));
                }
                _ => {
                    if let Some(p) = spec.preset {
                        c.check(
                            p >= spec.lo && p <= spec.hi,
                            &format!("{} preset {p} 落在 [{}, {}]", spec.name, spec.lo, spec.hi)
                        );
                        if spec.kind == GeneKind::Int {
                            c.check(p.fract() == 0.0, &format!("{} Int preset 为整值", spec.name));
                        }
                    }
                }
            }
            if let Some(y) = spec.year {
                c.check(y < 3, &format!("{} 分年下标合法", spec.name));
            }
        }
        // 名字唯一性单独断言（重复名会静默覆盖查表，必须显式拒绝）
        let mut names: Vec<&str> = GENE_SPECS.iter().map(|s| s.name).collect();
        names.sort_unstable();
        let dupes: Vec<&str> = names.windows(2).filter(|w| w[0] == w[1]).map(|w| w[0]).collect();
        println!("重复基因名: {dupes:?}");
        c.check(dupes.is_empty(), "基因名无重复");
        c.finish()
    }

    /// 解码语义：all_none → 全 None；all_preset → 逐基因 Some(preset)（不可单值
    /// 表示 preset 的基因保持 None）；区间端点/中点映射正确；Int 取整、Bool 分界。
    #[test]
    fn ga_decode_none_and_some_semantics() -> Result<()> {
        let mut c = Checks::new();

        // 1) all_none ≡ ParamOverride::all_none()（零差异编码保障）
        let none_ov = decode(&GaGenome::all_none())?;
        c.check(
            none_ov == ParamOverride::all_none(),
            "decode(all_none) == ParamOverride::all_none()"
        );

        // 2) all_preset 逐基因回环：Some(preset) / 不可单值 preset 的基因 None
        let preset_ov = decode(&GaGenome::all_preset())?;
        println!("preset 回环抽查: vital_rest={:?} caps={:?} wisdom_floor={:?} shining={:?}",
            preset_ov.vital_rest, preset_ov.friend_outing_cumulative_caps,
            preset_ov.wisdom_vital_floor, preset_ov.pt_tradeoff_shining
        );
        c.check(preset_ov.vital_rest == Some(40), "vital_rest 回环 = Some(40)");
        c.check(preset_ov.rest_target_vital == Some(55), "rest_target_vital 回环 = Some(55)");
        c.check(
            preset_ov.friend_outing_cumulative_caps == [Some(0), Some(2), Some(5)],
            "caps 回环 = [Some(0), Some(2), Some(5)]"
        );
        c.check(preset_ov.wisdom_vital_floor.is_none(), "wisdom_vital_floor 回环 = None（preset 不豁免不可单值表示）");
        c.check(preset_ov.pt_tradeoff_shining.is_none(), "pt_tradeoff_shining 回环 = None（动态 preset 不可单值表示）");
        c.check(preset_ov.eat_requires_training == Some(true), "eat_requires_training 回环 = Some(true)");
        c.check(preset_ov.dynamic_special_targets == Some(true), "dynamic_special_targets 回环 = Some(true)");
        c.check(preset_ov.status_gap_strength == Some(0.5), "status_gap_strength 回环 = Some(0.5)");
        c.check(preset_ov.ramen_window_weight == Some(0.10), "ramen_window_weight 回环 = Some(0.10)");

        // 2.5) preset≠Default 陷阱位：GA 基线必须锚在 preset 值，不是 Default。
        //      依据：RamenPolicyConfig::default() cap_discount_weight=0.0、
        //      LocalRamenConfig::default() dynamic_status_balance=false、
        //      effective_ramen_failure preset=false 而 Default=true（policy.rs / local_ramen_trainer.rs）。
        c.check(
            preset_ov.cap_discount_weight == Some(1.0),
            "cap_discount_weight 锚 = preset 1.0（Default 0.0=机制全关，严禁当基线）"
        );
        c.check(
            preset_ov.dynamic_status_balance == Some(true),
            "dynamic_status_balance 锚 = preset true（Default false）"
        );
        c.check(
            preset_ov.effective_ramen_failure == Some(false),
            "effective_ramen_failure 锚 = preset false（Default true，方向相反）"
        );

        // 3) 端点映射：全 1.0 基因 = 全 hi（Int 取整）
        let hi_genome = GaGenome(vec![1.0; GENE_COUNT]);
        let hi_ov = decode(&hi_genome)?;
        let vital_rest_spec = &GENE_SPECS[gene_index("vital_rest")?];
        let want_rest = vital_rest_spec.hi.round() as i32;
        println!("全 1 基因: vital_rest = {:?}（期望 {want_rest}）", hi_ov.vital_rest);
        c.check(hi_ov.vital_rest == Some(want_rest), "全 1 基因 vital_rest = Some(70)");
        // Bool 全 1 = true
        c.check(hi_ov.eat_requires_training == Some(true), "全 1 基因 eat_requires_training = Some(true)");

        // 4) 全 0 基因 = 全 lo；Bool 全 0 = false
        let lo_genome = GaGenome(vec![0.0; GENE_COUNT]);
        let lo_ov = decode(&lo_genome)?;
        c.check(lo_ov.vital_rest == Some(25), "全 0 基因 vital_rest = Some(25)");
        c.check(lo_ov.eat_requires_training == Some(false), "全 0 基因 eat_requires_training = Some(false)");

        // 5) 中点映射（Float）
        let mid_genome = GaGenome(vec![0.5; GENE_COUNT]);
        let mid_ov = decode(&mid_genome)?;
        let rate_spec = &GENE_SPECS[gene_index("status_rate")?];
        let want = (rate_spec.lo + 0.5 * (rate_spec.hi - rate_spec.lo)) as f32;
        println!("全 0.5 基因: status_rate = {:?}（期望 {want}）", mid_ov.status_rate);
        c.check(
            (mid_ov.status_rate.unwrap() - want).abs() < 1e-6,
            "全 0.5 基因 status_rate = 区间中点 1.25"
        );

        // 6) 随机基因组解码后 Some 值全部落在范围内（Int 整值）
        let mut rng = StdRng::seed_from_u64(99);
        for trial in 0..8 {
            let genes: Vec<f32> = (0..GENE_COUNT)
                .map(|_| if rng.random::<f64>() < 0.2 { rng.random_range(-1.0..0.0f32) } else { rng.random_range(0.0..=1.0f32) })
                .collect();
            let ov = decode(&GaGenome(genes))?;
            // spot 抽查 6 个代表字段（覆盖 Float/Int/Bool 三类）
            let checks = [
                ("vital_rest", ov.vital_rest.map(|v| v as f64), true),
                ("rest_target_vital", ov.rest_target_vital.map(|v| v as f64), true),
                ("shining_bonus", ov.shining_bonus.map(|v| v as f64), false),
                ("friend_outing_cumulative_caps_y2", ov.friend_outing_cumulative_caps[1].map(|v| v as f64), true),
                ("feeling_overflow_threshold", ov.feeling_overflow_threshold.map(|v| v as f64), true),
                ("ramen_window_weight", ov.ramen_window_weight.map(|v| v as f64), false)
            ];
            for (name, val, is_int) in checks {
                let spec = &GENE_SPECS[gene_index(name)?];
                if let Some(v) = val {
                    c.check(
                        v >= spec.lo && v <= spec.hi,
                        &format!("trial{trial} {name} = {v} 落在 [{}, {}]", spec.lo, spec.hi)
                    );
                    if is_int {
                        c.check(v.fract() == 0.0, &format!("trial{trial} {name} Int 取整"));
                    }
                }
            }
        }
        c.finish()
    }

    /// repair 约束链：6 条序关系逐一验证（含 None 保留语义）。
    #[test]
    fn ga_repair_chains() -> Result<()> {
        use preset_baseline as pb;
        let mut c = Checks::new();

        // 1. rest_target_vital >= vital_rest（抬右端）
        let mut ov = ParamOverride::all_none();
        ov.vital_rest = Some(40);
        ov.rest_target_vital = Some(30);
        repair_override(&mut ov);
        println!("规则1: rest_target {:?} (rest=40)", ov.rest_target_vital);
        c.check(ov.rest_target_vital == Some(40), "规则1: 休息目标被抬到硬门限");

        // 2. vital_rest_eating <= vital_rest（0 = 关闭豁免不动）
        let mut ov = ParamOverride::all_none();
        ov.vital_rest = Some(40);
        ov.vital_rest_eating = [Some(60), Some(0), None];
        repair_override(&mut ov);
        println!("规则2: eating = {:?}", ov.vital_rest_eating);
        c.check(ov.vital_rest_eating[0] == Some(40), "规则2: 吃面门限 60 裁到 40");
        c.check(ov.vital_rest_eating[1] == Some(0), "规则2: 0（关闭）保持不变");
        c.check(ov.vital_rest_eating[2].is_none(), "规则2: None（保留 preset）保持不变");

        // 3. wisdom_vital_floor < vital_rest（豁免带非空）
        let mut ov = ParamOverride::all_none();
        ov.vital_rest = Some(40);
        ov.wisdom_vital_floor = Some(50);
        repair_override(&mut ov);
        println!("规则3: floor = {:?}", ov.wisdom_vital_floor);
        c.check(ov.wisdom_vital_floor == Some(39), "规则3: floor >= rest 被裁到 rest-1");
        // floor < rest 的合法值不动
        let mut ov = ParamOverride::all_none();
        ov.vital_rest = Some(40);
        ov.wisdom_vital_floor = Some(30);
        repair_override(&mut ov);
        c.check(ov.wisdom_vital_floor == Some(30), "规则3: 合法 floor 不动");

        // 4. y3_post <= y3_pre
        let mut ov = ParamOverride::all_none();
        ov.y3_pre_train_vital_target = Some(30);
        ov.y3_post_train_vital_target = Some(40);
        repair_override(&mut ov);
        println!("规则4: post = {:?}", ov.y3_post_train_vital_target);
        c.check(ov.y3_post_train_vital_target == Some(30), "规则4: post 被裁到 pre");

        // 5. y3_hard <= y3_pre
        let mut ov = ParamOverride::all_none();
        ov.y3_pre_train_vital_target = Some(30);
        ov.y3_post_train_hard_floor = Some(45);
        repair_override(&mut ov);
        println!("规则5: hard = {:?}", ov.y3_post_train_hard_floor);
        c.check(ov.y3_post_train_hard_floor == Some(30), "规则5: hard 被裁到 pre");

        // 6. caps 单调不减且 <= 5；None 全 None 保持
        let mut ov = ParamOverride::all_none();
        ov.friend_outing_cumulative_caps = [Some(5), Some(1), Some(3)];
        repair_override(&mut ov);
        let caps = ov.friend_outing_cumulative_caps;
        println!("规则6: caps = {caps:?}");
        let (a, b, d) = (caps[0].unwrap(), caps[1].unwrap(), caps[2].unwrap());
        c.check(a <= b && b <= d, "规则6: 修复后单调不减");
        c.check(d <= 5, "规则6: 上限 <= 5");
        let mut ov = ParamOverride::all_none();
        repair_override(&mut ov);
        c.check(
            ov.friend_outing_cumulative_caps == [None, None, None],
            "规则6: 全 None caps 保持全 None（preset [0,2,5] 无需固化）"
        );

        // 端到端：decode 内部调用 repair——违规基因组解码即合法
        let mut genes = vec![-1.0f32; GENE_COUNT];
        genes[gene_index("vital_rest")?] = 1.0; // 70
        genes[gene_index("rest_target_vital")?] = 0.0; // 40 < 70 → 应被抬到 70
        let ov = decode(&GaGenome(genes))?;
        println!("端到端: rest={:?} target={:?}", ov.vital_rest, ov.rest_target_vital);
        c.check(
            ov.rest_target_vital == Some(70),
            "端到端: decode 后 rest_target >= vital_rest"
        );

        let _ = (pb::VITAL_REST, pb::REST_TARGET_VITAL);
        c.finish()
    }

    /// GA 主循环确定性：同 ga_seed 两次运行结果逐位一致（mock 评估器）。
    #[test]
    fn ga_operators_deterministic_with_mock() -> Result<()> {
        let params = GaParams {
            pop: 8,
            gens: 3,
            elitism: 2,
            tournament_k: 2,
            ga_seed: 7,
            ..GaParams::default()
        };
        let want_gens = params.gens;
        let mut e1 = MockEvaluator::new();
        let r1 = GaOptimizer::new(params.clone()).run(&mut e1)?;
        let mut e2 = MockEvaluator::new();
        let r2 = GaOptimizer::new(params).run(&mut e2)?;

        let mut c = Checks::new();
        println!("run1 best={} run2 best={}", r1.best_fitness, r2.best_fitness);
        c.check(
            (r1.best_fitness - r2.best_fitness).abs() < 1e-12,
            "同种子两次运行最优适应度一致"
        );
        c.check(r1.best_genome == r2.best_genome, "同种子两次运行最优基因组逐位一致");
        c.check(r1.history.len() == want_gens, "代数记录完整");
        c.check(
            r1.history
                .iter()
                .zip(r2.history.iter())
                .all(|(a, b)| (a.best_fitness - b.best_fitness).abs() < 1e-12 && a.full_count == b.full_count),
            "逐代摘要一致"
        );
        // 变异/交叉产出的基因始终 ∈ [-1, 1]（make_individual 解码成功即隐含验证）
        c.check(
            r1.history.iter().all(|h| h.sigma > 0.0),
            "每代 σ > 0"
        );
        println!("run1 history: {:?}", r1.history.iter().map(|h| (h.generation, h.best_fitness, h.full_count)).collect::<Vec<_>>());
        c.finish()
    }

    /// 缓存去重与两级晋升：同一 (key, level) 只实算一次；每代晋升 elitism 个精评。
    #[test]
    fn ga_cache_and_promotion_with_mock() -> Result<()> {
        let params = GaParams {
            pop: 4,
            gens: 2,
            elitism: 2,
            tournament_k: 2,
            ga_seed: 11,
            ..GaParams::default()
        };
        let want_elitism = params.elitism;
        let want_gens = params.gens;
        let mut e = MockEvaluator::new();
        let report = GaOptimizer::new(params).run(&mut e)?;

        let mut c = Checks::new();
        println!(
            "counts: screen={} full={} holdout={} cache_hits={}",
            e.counts.screen_evals, e.counts.full_evals, e.counts.holdout_evals, e.counts.cache_hits
        );
        // 直接验证缓存语义：同 key 重复评估命中缓存
        let genome = GaGenome::all_none();
        let comp_counts = bench::DEFAULT_COMP_COUNTS;
        let comp_idx = bench::DEFAULT_COMP_INDEX;
        let card_sel = CardSelection { indices: [0; 5], friend_idrank: crate::card_pool::FRIEND_IDRANK };
        let gh = genome_hash(&genome);
        let ch = super::comp_hash(comp_idx, &comp_counts);
        let sh = card_sel.hash_key();
        let key = gh ^ ch.wrapping_mul(0x9e3779b97f4a7c15) ^ sh.wrapping_mul(0x517cc1b727220a95);
        let ov = decode(&genome)?;
        let hits_before = e.counts.cache_hits;
        let _ = e.evaluate(key, &ov, &comp_counts, &card_sel, EvalLevel::Screen)?;
        c.check(e.counts.cache_hits == hits_before + 1, "重复 (key, level) 评估命中缓存");

        // 晋升语义：每代精评后至少 elitism 个个体持有精评卡（full_count 为累计持有数，
        // 子代重复亲本基因组或晋升名次与精英重叠时会更多，下界不变）
        for h in &report.history {
            c.check(
                h.full_count >= want_elitism,
                &format!("第{}代持有精评卡个体数 >= 精英数 {}（实际 {}）", h.generation + 1, want_elitism, h.full_count)
            );
        }
        // holdout 只对冠军：gens 代至多 gens 次（缓存命中不计）
        c.check(
            e.counts.holdout_evals <= want_gens,
            "holdout 评估次数 <= 代数"
        );
        // 全程最优必有精评卡
        c.check(report.best_card.level == EvalLevel::Full, "全程最优使用精评口径");
        c.check(
            (report.best_card.fitness - report.best_fitness).abs() < 1e-12,
            "全程最优适应度与精评卡一致"
        );
        // 过拟合哨兵已计算（holdout 均为 mock，gap = eval fitness - holdout fitness）
        c.check(report.overfit_gap.is_some(), "过拟合哨兵（eval-holdout gap）已计算");
        println!("overfit_gap = {:?}", report.overfit_gap);
        c.finish()
    }

    /// 明细行通道：evaluate 产生 (level, key, fitness, 行) 四元组，take 后清空。
    #[test]
    fn ga_detail_rows_flow_with_mock() -> Result<()> {
        let mut e = MockEvaluator::new();
        let genome = GaGenome::all_none();
        let comp_counts = bench::DEFAULT_COMP_COUNTS;
        let comp_idx = bench::DEFAULT_COMP_INDEX;
        let card_sel = CardSelection { indices: [0; 5], friend_idrank: crate::card_pool::FRIEND_IDRANK };
        let gh = genome_hash(&genome);
        let ch = super::comp_hash(comp_idx, &comp_counts);
        let sh = card_sel.hash_key();
        let key = gh ^ ch.wrapping_mul(0x9e3779b97f4a7c15) ^ sh.wrapping_mul(0x517cc1b727220a95);
        let ov = decode(&genome)?;
        e.evaluate(key, &ov, &comp_counts, &card_sel, EvalLevel::Screen)?;
        e.evaluate(key, &ov, &comp_counts, &card_sel, EvalLevel::Screen)?; // 缓存命中，不产明细
        let rows = e.take_detail_rows();
        let mut c = Checks::new();
        println!("明细行数 = {}", rows.len());
        c.check(rows.len() == 1, "同 key 二次评估不重复产明细");
        c.check(rows[0].0 == EvalLevel::Screen && rows[0].1 == key, "明细行携带 level 与 key");
        let rows2 = e.take_detail_rows();
        c.check(rows2.is_empty(), "take 后明细清空");
        c.finish()
    }

    /// 初筛代表 build 挑选：速/耐/智各取类型数量最大者，不足按声明序补齐。
    #[test]
    fn ga_select_screen_builds() -> Result<()> {
        let builds = vec![
            bench::DeckComposition { counts: [3, 1, 1, 0, 0], name: "speed".to_string() },
            bench::DeckComposition { counts: [1, 3, 0, 1, 0], name: "sta".to_string() },
            bench::DeckComposition { counts: [2, 1, 2, 0, 0], name: "power".to_string() },
            bench::DeckComposition { counts: [1, 1, 1, 2, 0], name: "guts".to_string() },
            bench::DeckComposition { counts: [1, 1, 1, 0, 2], name: "wis".to_string() }
        ];
        let picked = select_screen_builds(&builds, 3);
        println!("picked = {picked:?}");
        let mut c = Checks::new();
        c.check(picked == vec!["speed", "sta", "wis"], "速/耐/智各取一（speed/sta/wis）");
        let picked1 = select_screen_builds(&builds, 1);
        c.check(picked1 == vec!["speed"], "count=1 只取速主");
        // 声明序回退：全同分布取靠前
        let flat = vec![
            bench::DeckComposition { counts: [1, 1, 1, 1, 1], name: "a".to_string() },
            bench::DeckComposition { counts: [1, 1, 1, 1, 1], name: "b".to_string() }
        ];
        let picked_f = select_screen_builds(&flat, 3);
        println!("flat picked = {picked_f:?}");
        c.check(picked_f == vec!["a", "b"], "不足时按声明序补齐全部");
        c.finish()
    }

    /// 基因组哈希：同基因组同哈希、不同基因组哈希不同（抽样）。
    #[test]
    fn ga_genome_hash_distinct() -> Result<()> {
        let a = GaGenome::all_none();
        let b = GaGenome::all_preset();
        let mut c = Checks::new();
        c.check(genome_hash(&a) == genome_hash(&GaGenome::all_none()), "同基因组哈希一致");
        c.check(genome_hash(&a) != genome_hash(&b), "不同基因组哈希不同（all_none vs all_preset）");
        let mut d = a.clone();
        d.0[0] = 0.5;
        c.check(genome_hash(&a) != genome_hash(&d), "单基因差异即哈希不同");
        c.finish()
    }

    /// GA 零差异验证第二层（整局层，design.md §7）：全 None 基因组跑整局
    /// 必须与基线逐位一致——结局字段 + 决策日志。
    ///
    /// - 通道 A：`with_overrides(all_none)`（观测模式，与基线同模式对比，
    ///   含 score_breakdown；elapsed_us 为墙钟计时必须排除）；
    /// - 通道 B：`with_overrides_for_rollout(all_none)`（GA 批评估实际路径，
    ///   rollout 关闭观测，故对比除 score_breakdown 外的决策字段）。
    /// - 基线锚点：score = 92393、五维 = [3337,2445,2107,1230,1251]
    ///   （2026-09-17 fork master 875dd2c 合并后重录）。
    #[test]
    fn ga_zero_diff_full_game_all_none_bit_identical() -> Result<()> {
        bootstrap()?;

        // 基线：正式 preset（观测模式）
        let base_trainer = LoggingTrainer::new(RecommendedRamenTrainer::new(), 0);
        let base_out = bench::run_seeded(UMA, &DECK, &INHERIT, 42, 0, &base_trainer)?;
        let base_log = base_trainer.take_records();

        // 通道 A：with_overrides(all_none)
        let all_none = ParamOverride::all_none();
        let obs_trainer = LoggingTrainer::new(RecommendedRamenTrainer::with_overrides(&all_none), 0);
        let obs_out = bench::run_seeded(UMA, &DECK, &INHERIT, 42, 0, &obs_trainer)?;
        let obs_log = obs_trainer.take_records();

        // 通道 B：with_overrides_for_rollout(all_none)
        let ro_trainer =
            LoggingTrainer::new(RecommendedRamenTrainer::with_overrides_for_rollout(&all_none), 0);
        let ro_out = bench::run_seeded(UMA, &DECK, &INHERIT, 42, 0, &ro_trainer)?;
        let ro_log = ro_trainer.take_records();

        let mut c = Checks::new();
        println!("基线 score={} five={:?}", base_out.score, base_out.five_status);
        c.check(base_out.score == MASTER_ANCHOR_SCORE, "基线锚点 score = 92393（2026-09-17 fork master 875dd2c 合并后重录）");
        c.check(
            base_out.five_status == MASTER_ANCHOR_FIVE,
            "基线锚点五维 = [3337,2445,2107,1230,1251]（2026-09-17 fork master 875dd2c 合并后重录）"
        );

        let cmp_outcome = |c: &mut Checks, a: &GameOutcome, b: &GameOutcome, label: &str| {
            c.check(a.score == b.score, &format!("{label} score 逐位一致"));
            c.check(a.rank == b.rank, &format!("{label} rank 一致"));
            c.check(a.five_status == b.five_status, &format!("{label} 五维逐位一致"));
            c.check(a.skill_pt == b.skill_pt, &format!("{label} 技能点逐位一致"));
            c.check(
                a.yearly_scenario_pt == b.yearly_scenario_pt,
                &format!("{label} 逐年剧本 PT 逐位一致")
            );
            c.check(
                a.yearly_eat_count == b.yearly_eat_count,
                &format!("{label} 逐年吃面数逐位一致")
            );
            c.check(
                a.yearly_selected_regions == b.yearly_selected_regions,
                &format!("{label} 逐年地区逐位一致")
            );
            c.check(
                a.yearly_friend_turns == b.yearly_friend_turns,
                &format!("{label} 逐年友人回数逐位一致")
            );
            c.check(
                a.yearly_gauge_gain == b.yearly_gauge_gain && a.yearly_gauge_overflow == b.yearly_gauge_overflow,
                &format!("{label} 逐年量表逐位一致")
            );
            c.check(a.rmj_ok == b.rmj_ok, &format!("{label} RMJ 达成年数一致"));
            c.check(a.friend_all == b.friend_all, &format!("{label} friend_all 一致"));
            c.check(a.free_race_ok == b.free_race_ok, &format!("{label} free_race_ok 一致"));
        };
        cmp_outcome(&mut c, &base_out, &obs_out, "通道A(观测模式)");
        cmp_outcome(&mut c, &base_out, &ro_out, "通道B(rollout)");

        // 决策日志逐行：seed/turn/stage/candidates/action_index/action_desc 必查；
        // score_breakdown 仅观测模式可比（rollout 恒 None）；elapsed_us 墙钟计时排除。
        let cmp_logs = |c: &mut Checks,
                        a: &crate::output::decision_log::DecisionLog,
                        b: &crate::output::decision_log::DecisionLog,
                        with_breakdown: bool,
                        label: &str| {
            c.check(
                a.rows.len() == b.rows.len(),
                &format!("{label} 决策行数一致（{} vs {}）", a.rows.len(), b.rows.len())
            );
            let n = a.rows.len().min(b.rows.len());
            let mut first_diff = None;
            for i in 0..n {
                let (x, y) = (&a.rows[i], &b.rows[i]);
                let same = x.seed == y.seed
                    && x.turn == y.turn
                    && x.stage == y.stage
                    && x.candidates == y.candidates
                    && x.action_index == y.action_index
                    && x.action_desc == y.action_desc
                    && (!with_breakdown || x.score_breakdown == y.score_breakdown);
                if !same {
                    first_diff = Some(i);
                    break;
                }
            }
            if let Some(i) = first_diff {
                println!("{label} 首个差异行 {i}:\n  基线 = {:?}\n  覆盖 = {:?}", a.rows[i], b.rows[i]);
            }
            c.check(first_diff.is_none(), &format!("{label} 决策日志逐位一致（共 {n} 行）"));
        };
        cmp_logs(&mut c, &base_log, &obs_log, true, "通道A");
        cmp_logs(&mut c, &base_log, &ro_log, false, "通道B");
        c.finish()
    }

    /// 基因活性冒烟抽样（design.md §7.3）：代表性基因置 Some(显著偏离 preset)
    /// 后，整局决策序列必须与基线不同——证明覆盖层真正穿透到策略内核。
    #[test]
    fn ga_gene_activity_probe_sample() -> Result<()> {
        bootstrap()?;

        // 基线
        let base_trainer = LoggingTrainer::new(RecommendedRamenTrainer::new(), 0);
        let base_out = bench::run_seeded(UMA, &DECK, &INHERIT, 42, 0, &base_trainer)?;
        let base_log = base_trainer.take_records();

        // 探针基因组：all_none + 代表性基因推到区间端点
        let mut genome = GaGenome::all_none();
        for (name, g) in [
            ("vital_rest", 1.0f32),                        // Some(70) ≠ preset 40
            ("shining_bonus", 1.0),                        // Some(150) ≠ 60
            ("eat_requires_training", 0.0),                // Some(false) ≠ true
            ("friend_outing_cumulative_caps_y1", 1.0),     // [5,5,5] ≠ [0,2,5]
            ("friend_outing_cumulative_caps_y2", 1.0),
            ("friend_outing_cumulative_caps_y3", 1.0),
            ("pt_rate_y2", 1.0),                           // Some(96) ≠ 64
            ("wisdom_vital_floor", 1.0),                   // 启用豁免（repair 会裁到 rest-1）
            ("dynamic_status_balance", 0.0),               // Some(false)
            ("status_gap_strength", 1.0)                   // Some(2.0) ≠ 0.5
        ] {
            genome.0[gene_index(name)?] = g;
        }
        let ov = decode(&genome)?;
        println!(
            "探针覆盖抽查: vital_rest={:?} shining={:?} eat_req={:?} caps={:?} floor={:?}",
            ov.vital_rest, ov.shining_bonus, ov.eat_requires_training,
            ov.friend_outing_cumulative_caps, ov.wisdom_vital_floor
        );
        let probe_trainer =
            LoggingTrainer::new(RecommendedRamenTrainer::with_overrides_for_rollout(&ov), 0);
        let probe_out = bench::run_seeded(UMA, &DECK, &INHERIT, 42, 0, &probe_trainer)?;
        let probe_log = probe_trainer.take_records();

        let mut c = Checks::new();
        let outcome_differs =
            probe_out.score != base_out.score || probe_out.five_status != base_out.five_status;
        println!(
            "基线 score={} 探针 score={} 结局不同={outcome_differs}",
            base_out.score, probe_out.score
        );
        c.check(outcome_differs, "探针结局与基线不同（覆盖层生效）");
        let decisions_differ = probe_log.rows.len() != base_log.rows.len()
            || probe_log
                .rows
                .iter()
                .zip(base_log.rows.iter())
                .any(|(a, b)| a.action_index != b.action_index || a.action_desc != b.action_desc);
        c.check(decisions_differ, "探针决策序列与基线不同（策略内核被穿透）");
        // 修复链在探针上同样工作：floor 被裁到 rest-1 = 69
        c.check(
            ov.wisdom_vital_floor == Some(69),
            "探针 wisdom_vital_floor 经 repair 落在 rest-1 = 69"
        );
        c.finish()
    }

    /// 活体小规模冒烟：真实模拟评估器 + 真实 GA 主循环跑通全流程。
    ///
    /// 种群 3 / 2 代 / 每级 1 局——验证 SimFitnessEvaluator 与 GaOptimizer 的
    /// 端到端集成（缓存、晋升、holdout、明细落盘通道），不追求搜索质量。
    #[test]
    fn ga_live_smoke_small() -> Result<()> {
        bootstrap()?;
        println!("布局全枚举 = {} 种", bench::COMPOSITION_COUNT);
        println!("默认布局 = {:?} (idx={})", bench::DEFAULT_COMP_COUNTS, bench::DEFAULT_COMP_INDEX);

        let params = GaParams {
            pop: 3,
            gens: 2,
            elitism: 1,
            tournament_k: 2,
            screen_builds: 1,
            screen_runs: 1,
            full_builds: 1,
            full_runs: 1,
            holdout_runs: 1,
            ga_seed: 42,
            ..GaParams::default()
        };
        let pool = SsrPool::load()?;
        let mut evaluator =
            SimFitnessEvaluator::new(UMA, FRIEND, INHERIT, params.clone(), pool)?;
        let report = GaOptimizer::new(params).run(&mut evaluator)?;

        let mut c = Checks::new();
        println!(
            "最优 fitness={:.1} mean_score={:.1} counts={:?} 耗时={:.0}ms",
            report.best_fitness, report.best_card.mean_score, report.counts, report.elapsed_ms
        );
        c.check(report.history.len() == 2, "2 代全部完成");
        c.check(report.best_fitness.is_finite(), "最优适应度有限");
        c.check(report.best_card.level == EvalLevel::Full, "最优为精评口径");
        c.check(report.holdout_card.is_some(), "holdout 复验已执行");
        c.check(report.overfit_gap.is_some(), "过拟合哨兵已计算");
        c.check(
            evaluator.counts().screen_evals >= 3,
            "初筛评估至少覆盖全部初始个体"
        );
        c.check(
            evaluator.counts().full_evals >= 1,
            "全程至少 1 次精评（连任冠军命中缓存不重算，属预期）"
        );
        let details = evaluator.take_detail_rows();
        println!("明细行数 = {}", details.len());
        c.check(!details.is_empty(), "明细行通道有产出");
        // 明细行 CSV 列数与 ga_detail.csv 落盘表头一致
        // 2026-09-17 修正断言：14d756a 起 SimFitnessEvaluator 的明细行在
        // outcome_to_row（31 列标准行）之后追加 deck 字符串（ga_optimize.rs 落盘
        // 表头同为 RESULTS_HEADER + "deck"，主代码两处严格对齐）；本断言漏算
        // 该列导致误报，补上 +1。
        if let Some((_, _, _, row)) = details.first() {
            c.check(
                row.len() == bench::RESULTS_HEADER.len() + 1,
                "明细行列数 = RESULTS_HEADER + deck 列"
            );
        }
        // 全 None 个体（初始个体 0）在真实评估里 fitness 应等于其 mean_score
        // （无比赛失败、RMJ 全成时无惩罚项；若有惩罚则 fitness < mean_score，
        // 这里只验证有限性与自洽，不锁定具体分数）
        c.finish()
    }
    /// 布局交叉/变异产物合法性：合计 5、单类 ≤ 3、索引 ∈ [0, 101)。
    #[test]
    fn ga_composition_operators_legal() -> Result<()> {
        let mut rng = StdRng::seed_from_u64(777);
        let table = bench::all_compositions();
        let mut c = Checks::new();

        // 交叉：100 次随机交叉，产物索引必在 [0, 101)
        for _ in 0..100 {
            let a = rng.random_range(0..bench::COMPOSITION_COUNT);
            let b = rng.random_range(0..bench::COMPOSITION_COUNT);
            let child = GaOptimizer::crossover_comp(a, b, &mut rng);
            c.check(child < bench::COMPOSITION_COUNT, &format!("交叉产物 {child} < 101"));
            let counts = table[child];
            c.check(counts.iter().sum::<usize>() == 5, &format!("交叉产物合计 = 5"));
            c.check(counts.iter().all(|&x| x <= 3), &format!("交叉产物单类 ≤ 3"));
        }

        // 变异：100 次随机变异，产物 ≠ 原值（变异发生时）且合法
        for _ in 0..100 {
            let orig = rng.random_range(0..bench::COMPOSITION_COUNT);
            let mutated = GaOptimizer::mutate_comp(orig, &mut rng);
            c.check(mutated < bench::COMPOSITION_COUNT, &format!("变异产物 {mutated} < 101"));
            if mutated != orig {
                let counts = table[mutated];
                c.check(counts.iter().sum::<usize>() == 5, "变异产物合计 = 5");
                c.check(counts.iter().all(|&x| x <= 3), "变异产物单类 ≤ 3");
            }
        }
        c.finish()
    }

    /// 三维哈希区分：不同 (genome, comp, card_sel) → 不同 key。
    #[test]
    fn ga_three_dim_hash_distinct() -> Result<()> {
        let genome_a = GaGenome::all_none();
        let genome_b = GaGenome::all_preset();
        let card_sel_a = CardSelection { indices: [0; 5], friend_idrank: crate::card_pool::FRIEND_IDRANK };
        let card_sel_b = CardSelection { indices: [1, 0, 0, 0, 0], friend_idrank: crate::card_pool::FRIEND_IDRANK };
        let comp_a = bench::DEFAULT_COMP_INDEX;
        let comp_b = 0usize; // [0,0,0,2,3] or similar

        let mut c = Checks::new();

        // 只改 genome
        let gh_a = genome_hash(&genome_a);
        let gh_b = genome_hash(&genome_b);
        c.check(gh_a != gh_b, "不同 genome → 不同 genome_hash");

        // 只改 comp
        let comp_counts_a = bench::all_compositions()[comp_a];
        let comp_counts_b = bench::all_compositions()[comp_b];
        let ch_a = comp_hash(comp_a, &comp_counts_a);
        let ch_b = comp_hash(comp_b, &comp_counts_b);
        c.check(ch_a != ch_b, "不同 comp → 不同 comp_hash");

        // 只改 card_sel
        c.check(card_sel_a.hash_key() != card_sel_b.hash_key(), "不同 card_sel → 不同 hash");

        // 完整 key：改任一维度即不同
        let sh = card_sel_a.hash_key();
        let key_aaa = gh_a ^ ch_a.wrapping_mul(0x9e3779b97f4a7c15) ^ sh.wrapping_mul(0x517cc1b727220a95);
        let key_baa = gh_b ^ ch_a.wrapping_mul(0x9e3779b97f4a7c15) ^ sh.wrapping_mul(0x517cc1b727220a95);
        let key_aba = gh_a ^ ch_b.wrapping_mul(0x9e3779b97f4a7c15) ^ sh.wrapping_mul(0x517cc1b727220a95);
        let sh_b = card_sel_b.hash_key();
        let key_aab = gh_a ^ ch_a.wrapping_mul(0x9e3779b97f4a7c15) ^ sh_b.wrapping_mul(0x517cc1b727220a95);
        c.check(key_aaa != key_baa, "改 genome → 不同 key");
        c.check(key_aaa != key_aba, "改 comp → 不同 key");
        c.check(key_aaa != key_aab, "改 card_sel → 不同 key");
        c.finish()
    }

    /// 默认布局+默认配卡 → build_deck 产出与旧行为一致（6张不重复，末位友人）。
    #[test]
    fn ga_default_layout_default_cards_zero_diff() -> Result<()> {
        bootstrap()?;
        let pool = SsrPool::load()?;
        let comp_counts = bench::DEFAULT_COMP_COUNTS; // [3, 1, 0, 0, 1]
        let card_sel = CardSelection::default_top(&pool);
        let deck = card_sel.build_deck(&pool, &comp_counts)?;

        let mut c = Checks::new();
        c.check(deck.len() == 6, "卡组 6 张");
        c.check(deck[5] == crate::card_pool::FRIEND_IDRANK, "末位 = 友人 303054");
        // 6 张卡 card_id 不重复
        let mut ids: Vec<u32> = deck.iter().map(|&id| id / 10).collect();
        let orig_len = ids.len();
        ids.sort();
        ids.dedup();
        c.check(ids.len() == orig_len, "卡组无重复 card_id");
        // 布局合法性
        c.check(comp_counts.iter().sum::<usize>() == 5, "默认布局合计 = 5");
        c.check(comp_counts.iter().all(|&x| x <= 3), "默认布局单类 ≤ 3");
        println!("默认布局卡组: {:?}", deck);
        c.finish()
    }

}
