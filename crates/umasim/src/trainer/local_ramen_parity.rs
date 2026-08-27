//! 配对指标迭代用训练器包装（深水矿脉版）。
//!
//! 与上游 `RecommendedRamenTrainer` 的关系：本文件按上游 preset 的**原始配方**
//! 等价重建三年策略实例，再叠加本 workbench 锁定的增量与深水矿脉覆盖——
//! 这样无需改动上游 `local_ramen_trainer.rs`（130KB 大文件，fork 内不可整写），
//! 就能扫描 cook2 / y3 门禁 / 友人权重 / 方案E折扣这些从未在当前锁定栈上复扫过的参数。
//!
//! 保真约定：重建的 `make_year()` 必须与上游 `RecommendedRamenTrainer::new()` 内联
//! 配方逐字段一致；上游 preset 演进时同步更新本文件并在决策日志注明。
//!
//! == 决策日志（配对局数 300、基线上游 ramen_workbench @ a6e1f48）==
//! - 动态属性平衡 gap=0.75 / overflow=1.00；邻域收缩确认该轴已收口
//! - 牺牲上限：老卡组 260 达峰（r800 +113.7 三绿）；比赛卡组 180
//! - 交互锁定：老卡组冠军 = `hint9-res60`（r800 三绿）；比赛卡组 =
//!   `sac180-win200-hint9-res60`（r800 三绿）；bond12 组合塌缩弃用
//! - 本轮新开：cook2 / y3门禁三件套 / stv×prw / capd 五条深水矿脉，token 见下
//!
//! 默认＝老卡组冠军全量固化。`RAMEN_VARIANT` 用绝对值覆盖 token：
//! - 基础层：`gapNNN` `ovNNN` `winNNN` `sacNNN` `rwcNNN` `bondN` `hintN` `resN` `pt32`
//! - 深水层：`cookNN` `y3preN` `y3sfNNN` `y3hardN` `stvNNN` `prwNNN` `capdNN`
//! - 选手切换：比赛卡组加 `sac180-win200`
//! 未声明字段落回默认；未知 token 直接报错。

use anyhow::{anyhow, Result};
use rand::prelude::StdRng;

use crate::{
    game::{
        ramen::{
            Operation,
            policy::RamenPolicyConfig,
            {RamenAction, RamenGame}
        },
        Trainer
    },
    gamedata::{EventChoice, EventData},
    trainer::local_ramen_trainer::LocalRamenTrainer
};

/// 单年份深水矿脉可调覆盖值；`apply()` 把全部当前态写到给定的年度实例上。
#[derive(Debug, Clone)]
pub struct VeinOverrides {
    // ==== 已锁定层（老卡组冠军为默认）====
    /// 动态属性平衡：短板追赶强度。
    pub status_gap_strength: f32,
    /// 动态属性平衡：近上限衰减强度。
    pub status_overflow_strength: f32,
    /// 长期结构牺牲上限。
    pub max_base_score_sacrifice: f32,
    /// 吃面训练窗口权重。
    pub ramen_window_weight: f32,
    /// 属性预留目标。
    pub status_reserve_max: f32,
    /// 早期羁绊价值。
    pub early_bond_value: f32,
    /// Hint 加成偏好。
    pub hint_bonus: f32,
    /// 地区弱位覆盖加分权重。
    pub region_weak_cover_weight: f32,
    /// 吃面后必须训练覆盖位（永久 true）。
    pub eat_requires_covered_train: bool,

    // ==== 深水矿脉层（数值＝上游 preset 当前值）====
    /// Cook2 库存凹函数估值总权重（上游 40）。
    pub cook2_stock_weight: f32,
    /// 第三年吃面前体力软目标（上游 25；每年决策都评估）。
    pub y3_pre_train_vital_target: i32,
    /// 第三年缺口软成本每点（上游 0.5）。
    pub y3_vital_shortfall_weight: f32,
    /// 非智力训练后硬底线（上游 15）。
    pub y3_post_train_hard_floor: i32,
    /// 友人隐藏风味饥饿加成权重（上游 300）。
    pub friend_hidden_starve_weight: f32,
    /// 友人主动积极使用固定加分（上游 150）。
    pub friend_proactive_weight: f32,
    /// 残余收益折扣（方案 E）权重（上游 1.0）。
    pub cap_discount_weight: f32,

    /// 三年技能 PT 权重。
    pub pt_rates: [f32; 3]
}

impl Default for VeinOverrides {
    fn default() -> Self {
        Self {
            status_gap_strength: 0.75,
            status_overflow_strength: 1.00,
            max_base_score_sacrifice: 260.0,
            ramen_window_weight: 0.10,
            status_reserve_max: 60.0,
            early_bond_value: 8.0,
            hint_bonus: 9.0,
            region_weak_cover_weight: 0.0,
            eat_requires_covered_train: true,
            cook2_stock_weight: 40.0,
            y3_pre_train_vital_target: 25,
            y3_vital_shortfall_weight: 0.5,
            y3_post_train_hard_floor: 15,
            friend_hidden_starve_weight: 300.0,
            friend_proactive_weight: 150.0,
            cap_discount_weight: 1.0,
            pt_rates: [16.0, 64.0, 64.0]
        }
    }
}

impl VeinOverrides {
    /// 把全部覆盖值写入一个年度实例；调用方负责上游 preset 等价底座已就位。
    fn apply_to(&self, year: &mut LocalRamenTrainer) {
        let c = &mut *year.config_mut();
        let p = &mut *year.policy_config_mut();
        p.region_weak_cover_weight = self.region_weak_cover_weight;
        p.cap_discount_weight = self.cap_discount_weight;
        c.dynamic_status_balance = self.status_gap_strength != 0.0 || self.status_overflow_strength != 0.0;
        c.status_gap_strength = self.status_gap_strength;
        c.status_overflow_strength = self.status_overflow_strength;
        c.max_base_score_sacrifice = self.max_base_score_sacrifice;
        c.ramen_window_weight = self.ramen_window_weight;
        c.status_reserve_max = self.status_reserve_max;
        c.early_bond_value = self.early_bond_value;
        c.hint_bonus = self.hint_bonus;
        c.eat_requires_covered_train = self.eat_requires_covered_train;
        c.cook2_stock_weight = self.cook2_stock_weight;
        c.y3_pre_train_vital_target = self.y3_pre_train_vital_target;
        c.y3_vital_shortfall_weight = self.y3_vital_shortfall_weight;
        c.y3_post_train_hard_floor = self.y3_post_train_hard_floor;
        c.friend_hidden_starve_weight = self.friend_hidden_starve_weight;
        c.friend_proactive_weight = self.friend_proactive_weight;
    }
}

/// 等价重建上游 preset 的单年实例（逐字段对齐 `RecommendedRamenTrainer::new()`），
/// 随后应用 [`VeinOverrides`]。`eating_rest` 仅第三年为 0（Y3 吃面必成放掉门限）。
fn make_year(pt_rate: f32, vital_rest: i32, eating_rest: i32, ov: &VeinOverrides) -> LocalRamenTrainer {
    use crate::trainer::local_ramen_trainer::LocalRamenConfig;

    let mut policy = RamenPolicyConfig::default();
    policy.pt_rate = pt_rate;
    policy.ramen_pt_weight = 2.0;
    policy.vital_rest = vital_rest;
    policy.vital_rest_eating = eating_rest;
    // 上游 preset：保守风险预算只用基础失败率打分，规则层仍用真实失败率。
    policy.effective_ramen_failure = false;

    let mut local = LocalRamenConfig::default();
    local.status_reserve_max = 40.0;
    local.dynamic_vital = true;
    local.probabilistic_hint = true;
    local.expected_fail = true;
    local.max_base_score_sacrifice = 140.0;
    local.ramen_window_weight = 0.10;
    local.ramen_train_coupling_weight = 2.0;
    local.eat_guarantee_weight = 3.0;
    local.friend_hidden_starve_weight = 300.0;
    local.friend_proactive_weight = 150.0;
    local.friend_future_hidden_weight = 0.0;
    local.dynamic_status_balance = true;
    local.status_gap_strength = 0.5;
    local.status_overflow_strength = 0.5;
    local.ramen_lookahead_weight = 0.0;
    local.ramen_lookahead_samples = 1;
    local.effective_ramen_failure = false;
    local.cook2_stock_weight = 40.0;
    local.eat_requires_training = true;
    local.eat_requires_covered_train = true;
    local.y3_pre_train_vital_target = 25;
    local.y3_post_train_vital_target = 0;
    local.y3_vital_shortfall_weight = 0.5;
    local.y3_post_train_hard_floor = 15;
    local.y3_recovery_horizon = true;
    local.friend_outing_replaces_rest = true;
    local.friend_outing3_recovery_vital = 0;
    // v44 千局验证胜出的友人跨年节奏，已晋级并受上游守门测试保护。
    local.friend_outing_cumulative_caps = [0, 2, 5];
    local.friend_rest_max_special = 4;
    local.deadline_urgency_scale = 0.0;
    local.dynamic_special_targets = true;

    let mut year = LocalRamenTrainer::with_configs(policy, local);
    ov.apply_to(&mut year);
    year
}

/// 从正式 preset 等价重建、按 `RAMEN_VARIANT` 覆盖的训练器。
pub struct IterationRamenTrainer {
    years: [LocalRamenTrainer; 3],
    /// 解析出的变体标签（供日志与测试断言）。
    pub variant: String
}

impl IterationRamenTrainer {
    /// 默认：两卡组锁定冠军配置；存在 `RAMEN_VARIANT` 时按 token 覆盖。
    pub fn new() -> Self {
        let variant = std::env::var("RAMEN_VARIANT").unwrap_or_default();
        match Self::from_variant(&variant) {
            Ok(t) => t,
            Err(e) => panic!("RAMEN_VARIANT 无效: {e}")
        }
    }

    /// 按 token 串构造；空串即老卡组冠军版。
    pub fn from_variant(variant: &str) -> Result<Self> {
        let mut ov = VeinOverrides::default();

        for token in variant.split('-').filter(|t| !t.is_empty()) {
            if token == "pt32" {
                ov.pt_rates = [32.0, 32.0, 32.0];
            } else if let Some(pct) = token.strip_prefix("gap") {
                ov.status_gap_strength = Self::parse_percent(token, pct)?;
            } else if let Some(pct) = token.strip_prefix("ov") {
                ov.status_overflow_strength = Self::parse_percent(token, pct)?;
            } else if let Some(per_mille) = token.strip_prefix("win") {
                ov.ramen_window_weight = Self::parse_per_mille(token, per_mille)?;
            } else if let Some(raw) = token.strip_prefix("sac") {
                ov.max_base_score_sacrifice =
                    raw.parse().map_err(|_| anyhow!("token {token} 数值段非法: {raw}"))?;
            } else if let Some(raw) = token.strip_prefix("rwc") {
                ov.region_weak_cover_weight =
                    raw.parse().map_err(|_| anyhow!("token {token} 数值段非法: {raw}"))?;
            } else if let Some(tenths) = token.strip_prefix("bond") {
                ov.early_bond_value =
                    tenths.parse::<f32>().map_err(|_| anyhow!("token {token} 数值段非法: {tenths}"))? / 10.0;
            } else if let Some(tenths) = token.strip_prefix("hint") {
                ov.hint_bonus =
                    tenths.parse::<f32>().map_err(|_| anyhow!("token {token} 数值段非法: {tenths}"))? / 10.0;
            } else if let Some(raw) = token.strip_prefix("res") {
                ov.status_reserve_max =
                    raw.parse().map_err(|_| anyhow!("token {token} 数值段非法: {raw}"))?;
            } else if let Some(raw) = token.strip_prefix("cook") {
                ov.cook2_stock_weight =
                    raw.parse().map_err(|_| anyhow!("token {token} 数值段非法: {raw}"))?;
            } else if let Some(raw) = token.strip_prefix("y3pre") {
                ov.y3_pre_train_vital_target =
                    raw.parse().map_err(|_| anyhow!("token {token} 数值段非法: {raw}"))?;
            } else if let Some(hundredths) = token.strip_prefix("y3sf") {
                ov.y3_vital_shortfall_weight = hundredths
                    .parse::<f32>()
                    .map_err(|_| anyhow!("token {token} 数值段非法: {hundredths}"))?
                    / 100.0;
            } else if let Some(raw) = token.strip_prefix("y3hard") {
                ov.y3_post_train_hard_floor =
                    raw.parse().map_err(|_| anyhow!("token {token} 数值段非法: {raw}"))?;
            } else if let Some(raw) = token.strip_prefix("stv") {
                ov.friend_hidden_starve_weight =
                    raw.parse().map_err(|_| anyhow!("token {token} 数值段非法: {raw}"))?;
            } else if let Some(raw) = token.strip_prefix("prw") {
                ov.friend_proactive_weight =
                    raw.parse().map_err(|_| anyhow!("token {token} 数值段非法: {raw}"))?;
            } else if let Some(hundredths) = token.strip_prefix("capd") {
                ov.cap_discount_weight = hundredths
                    .parse::<f32>()
                    .map_err(|_| anyhow!("token {token} 数值段非法: {hundredths}"))?
                    / 100.0;
            } else {
                return Err(anyhow!("未知 RAMEN_VARIANT token: {token}"));
            }
        }

        Ok(Self {
            // 上游 preset 门限节奏：不吃面回合三年一律 40；
            // 吃面回合仅第三年放掉（fail_rate_drop=100% 必成）。
            years: [
                make_year(ov.pt_rates[0], 40, 40, &ov),
                make_year(ov.pt_rates[1], 40, 40, &ov),
                make_year(ov.pt_rates[2], 40, 0, &ov)
            ],
            variant: variant.to_string()
        })
    }

    /// 解析 NNN% 数值后缀（`gap100` → 1.00）；非法后缀或越界报错。
    fn parse_percent(token: &str, pct: &str) -> Result<f32> {
        let pct_value: f32 = pct.parse().map_err(|_| anyhow!("token {token} 数值段非法: {pct}"))?;
        let value = pct_value / 100.0;
        if !(0.0..=2.0).contains(&value) {
            return Err(anyhow!("token {token} 超出允许区间 [0%,200%]"));
        }
        Ok(value)
    }

    /// 解析 NNN 数值后缀映射到千分比（`win150` → 0.15）；非法后缀报错。
    fn parse_per_mille(token: &str, per_mille: &str) -> Result<f32> {
        let value: f32 =
            per_mille.parse().map_err(|_| anyhow!("token {token} 数值段非法: {per_mille}"))?;
        Ok(value / 1000.0)
    }

    fn current_year(game: &RamenGame) -> usize {
        if game.turn() < 24 {
            0
        } else if game.turn() < 48 {
            1
        } else {
            2
        }
    }
}

impl Trainer<RamenGame> for IterationRamenTrainer {
    fn select_action(&self, game: &RamenGame, actions: &[RamenAction], rng: &mut StdRng) -> Result<usize> {
        self.years[Self::current_year(game)].select_action(game, actions, rng)
    }

    fn select_choice(&self, game: &RamenGame, choices: &[Vec<EventChoice>], rng: &mut StdRng) -> Result<usize> {
        self.years[Self::current_year(game)].select_choice(game, choices, rng)
    }

    fn select_event_choice(
        &self, game: &RamenGame, event: &EventData, choices: &[Vec<EventChoice>], rng: &mut StdRng
    ) -> Result<usize> {
        self.years[Self::current_year(game)].select_event_choice(game, event, choices, rng)
    }

    fn last_breakdown(&self) -> Option<String> {
        None
    }
}

/// 兼容旧名的历史别名。
pub type RestoredRamenTrainer = IterationRamenTrainer;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_variant_token_fails_cleanly() {
        assert!(IterationRamenTrainer::from_variant("nonsense").is_err());
        assert!(IterationRamenTrainer::from_variant("gapx100").is_err());
        assert!(IterationRamenTrainer::from_variant("gap250").is_err());
        assert!(IterationRamenTrainer::from_variant("cookxx").is_err());
        assert!(IterationRamenTrainer::from_variant("capdx").is_err());
    }

    #[test]
    fn locked_recipes_parse_cleanly() {
        assert!(IterationRamenTrainer::from_variant("").is_ok());
        for v in [
            "sac180-win200",
            "sac230-win200",
            "hint9-res60",
            "sac180-win200-hint9-res60",
            "pt32"
        ] {
            assert!(IterationRamenTrainer::from_variant(v).is_ok(), "锁定配方应可解析: {v}");
        }
        assert_eq!(
            IterationRamenTrainer::from_variant("sac180-win200-hint9-res60").unwrap().variant,
            "sac180-win200-hint9-res60"
        );
    }

    #[test]
    fn deep_vein_tokens_parse_cleanly() {
        // 深水矿脉 token：顺序无关、可任意组合。
        for v in [
            "cook55",
            "y3pre15-y3sf25-y3hard10",
            "stv400",
            "prw250",
            "capd80",
            "hint9-res60-cook55",
            "sac180-win200-hint9-res60-prw250"
        ] {
            assert!(
                IterationRamenTrainer::from_variant(v).is_ok(),
                "深水组合 token 应可解析: {v}"
            );
        }
    }

    /// 等价重建底座：默认构造必须保留上游 preset 的结构特征
    /// （友人 0/2/5 节奏、动态特殊目标、吃面必训覆盖位）。守门锚点防漂移。
    #[test]
    fn rebuilt_preset_keeps_structural_defaults() {
        let t = IterationRamenTrainer::from_variant("").unwrap();
        for year in &t.years {
            assert_eq!(year.config_ref().friend_outing_cumulative_caps, [0, 2, 5]);
            assert!(year.config_ref().dynamic_special_targets);
            assert!(year.config_ref().eat_requires_covered_train);
            assert!(year.config_ref().friend_outing_replaces_rest);
            assert_eq!(year.policy_config_ref().ramen_pt_weight, 2.0);
        }
    }
}
