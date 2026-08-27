//! 配对指标迭代用训练器包装。
//!
//! 锁定决策日志（配对局数 300、基线上游 ramen_workbench @ a6e1f48）：
//! - 动态属性平衡 gap=0.75 / overflow=1.00（run 33050390060，三绿）
//! - 邻域收缩确认（run 33051023023）：(0.75, 1.0) 为该机制局部峰值，轴关闭
//! - 牺牲上限梯度：老卡组 **260 达峰**（800 局复验 +113.7 三绿，run 33067654585）
//! - 交互批次：老卡组冠军 = `hint9-res60`（r800 +115.0/+102.1/+6.6，run 33071358939）；
//!   bond12 单飞虽最佳但组合中 PT 塌缩，弃用
//! - 比赛卡组：sac180-win200 胜出 A/B 后叠 `hint9-res60`
//!   （r800 +60.9/+53.8/+5.1，run 33071358939），配方 = `sac180-win200-hint9-res60`
//! - pt32 / win 轴微调 / rwc：历轮全负或无增益，永久关闭
//!
//! 卡组专用配方（在默认之上用 token 叠加；默认已含 hint9-res60 冠军项）：
//! - `2速1耐2智`（counts=21002）＝本文件默认
//! - `2速1力1根1智`（counts=20111）＝叠加 `sac180-win200`（覆盖 sac/window，其余继承）
//!
//! 默认＝上游 preset ＋ 已验证增益。环境变量 `RAMEN_VARIANT` 用带数值后缀的 token
//! 做**绝对值覆盖**，可自由组合：
//! - `gapNNN` 短板追赶（默认75）；`ovNNN` 近上限衰减（默认100）；`winNNN` 窗口权重 /1000（默认100）
//! - `sacNNN` 牺牲上限（老卡组默认260）；`rwcNNN` 地区弱位覆盖加分（默认0）
//! - `bondN` 前期羁绊（默认8）；`hintN` Hint 加成（老卡组默认9）；`resN` 年度保留体力下限（默认60）
//! - `pt32` 分年 PT 权重 32/32/32（已知负收益，仅复验用）
//! 二级矿脉（v44 深水参数，扫描用；数值＝当前锁定态的绝对值覆盖）：
//! - `cookNN` Cook2 库存凹函数估值总权重（锁定40）
//! - `y3preN` Y3 吃面前软目标（锁定25）；`y3sfNNN` Y3 缺口软成本 /100（锁定50）；`y3hardN` 非智硬底线（锁定15）
//! - `stvNNN` 友人饥饿加成权重（锁定300）；`prwNNN` 友人主动使用固定加分（锁定150）
//! - `capdNN` 残余收益折扣方案E权重 /100（锁定100 即 1.0）
//! 未声明字段落回默认；未知 token 直接报错，防止实验漂移。

use anyhow::{anyhow, Result};
use rand::prelude::StdRng;

use crate::{
    game::{
        ramen::{RamenAction, RamenGame},
        Trainer
    },
    gamedata::{EventChoice, EventData},
    trainer::local_ramen_trainer::{ExperimentOverrides, RecommendedRamenTrainer as PresetRamenTrainer}
};

/// 从正式 preset 复制、按 `RAMEN_VARIANT` 覆盖少量参数的训练器。
pub struct IterationRamenTrainer {
    inner: PresetRamenTrainer,
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

    /// 按 token 串构造；空串即老卡组冠军版（hint9-res60 全量固化）。
    pub fn from_variant(variant: &str) -> Result<Self> {
        // 默认＝workbench 锁定冠军（上游 preset ＋ 本分支已验证增益）。
        let mut ov = ExperimentOverrides {
            status_gap_strength: 0.75,
            status_overflow_strength: 1.00,
            max_base_score_sacrifice: 260.0,
            ramen_window_weight: 0.10,
            status_reserve_max: 60.0,
            early_bond_value: 8.0,
            hint_bonus: 9.0,
            ..ExperimentOverrides::default()
        };

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
                ov.y3_vital_shortfall_weight =
                    hundredths.parse::<f32>().map_err(|_| anyhow!("token {token} 数值段非法: {hundredths}"))?
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
                ov.cap_discount_weight =
                    hundredths.parse::<f32>().map_err(|_| anyhow!("token {token} 数值段非法: {hundredths}"))?
                        / 100.0;
            } else {
                return Err(anyhow!("未知 RAMEN_VARIANT token: {token}"));
            }
        }

        Ok(Self {
            inner: PresetRamenTrainer::with_experiment_overrides(&ov),
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
}

impl Trainer<RamenGame> for IterationRamenTrainer {
    fn select_action(&self, game: &RamenGame, actions: &[RamenAction], rng: &mut StdRng) -> Result<usize> {
        self.inner.select_action(game, actions, rng)
    }

    fn select_choice(&self, game: &RamenGame, choices: &[Vec<EventChoice>], rng: &mut StdRng) -> Result<usize> {
        self.inner.select_choice(game, choices, rng)
    }

    fn select_event_choice(
        &self, game: &RamenGame, event: &EventData, choices: &[Vec<EventChoice>], rng: &mut StdRng
    ) -> Result<usize> {
        self.inner.select_event_choice(game, event, choices, rng)
    }

    fn last_breakdown(&self) -> Option<String> {
        self.inner.last_breakdown()
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
        // 非法数值段与越界都必须走错误路径而不是 panic。
        assert!(IterationRamenTrainer::from_variant("gapx100").is_err());
        assert!(IterationRamenTrainer::from_variant("gap250").is_err());
        assert!(IterationRamenTrainer::from_variant("cookxx").is_err());
    }

    #[test]
    fn locked_recipes_parse_cleanly() {
        // 两套卡组的最终配方必须都能干净解析（绝对值覆盖，顺序无关）。
        assert!(IterationRamenTrainer::from_variant("").is_ok());
        for v in ["sac180-win200", "sac230-win200", "hint9-res60", "sac180-win200-hint9-res60"] {
            assert!(
                IterationRamenTrainer::from_variant(v).is_ok(),
                "锁定配方应可解析: {v}"
            );
        }
        assert_eq!(
            IterationRamenTrainer::from_variant("sac180-win200-hint9-res60")
                .unwrap()
                .variant,
            "sac180-win200-hint9-res60"
        );
    }

    #[test]
    fn deep_vein_tokens_parse_cleanly() {
        // v44 深水矿脉二级 token：老卡组与比赛卡组共用同一解析器，
        // 组合顺序无关、全部可解析。
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
        // 残缺数值段必须报错而不是吞掉。
        assert!(IterationRamenTrainer::from_variant("stvhigh").is_err());
        assert!(IterationRamenTrainer::from_variant("capdx").is_err());
    }
}
