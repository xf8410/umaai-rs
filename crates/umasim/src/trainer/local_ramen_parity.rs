//! 配对指标迭代用训练器包装。
//!
//! 锁定决策日志（配对局数 300、基线上游 ramen_workbench @ a6e1f48）：
//! - 动态属性平衡 gap=0.75 / overflow=1.00（run 33050390060，三绿）
//! - 邻域收缩确认（run 33051023023）：(0.75, 1.0) 为该机制局部峰值，轴关闭
//! - 牺牲上限梯度（run 33053124071 → 33065858798 → 33067050082）：
//!   140→180→200→220→240 单调上升，**260 达峰**（总分 +146.9 / 属性 +109.0 /
//!   PT +16.230），280 回落至 +144.6——峰值确认，已固化为下述默认值
//! - PT 权重 32 均匀档在 2速1耐2智 与 2速1力1根1智 双卡组均全负，永久关闭
//!
//! 卡组专用配方（在默认之上用 token 叠加）：
//! - `2速1耐2智`（counts=21002）＝本文件默认即可，无需 token
//! - `2速1力1根1智`（counts=20111）＝`sac230-win200`
//!   （run 33067050082：总分 +123.257 / 属性 +109.550 / PT +1.970；
//!    235 起 PT 过悬崖 −0.033，故此为严格三项正的最优格）
//!
//! 默认＝上游 preset ＋ 上述已验证增益（sac260 已并入）。环境变量
//! `RAMEN_VARIANT` 用带数值后缀的 token 做**绝对值覆盖**，可自由组合：
//! - `gapNNN`   短板追赶强度 NNN%（默认 75）
//! - `ovNNN`    近上限衰减强度 NNN%（默认 100）
//! - `winNNN`   吃面训练窗口权重 NNN/1000（默认 100，即 0.10）
//! - `sacNNN`   长期结构牺牲上限 NNN（默认 260，原上游 preset 为 140）
//! - `rwcNNN`   地区弱位覆盖加分 NNN（默认 0）
//! - `pt32`     分年技能 PT 权重 32/32/32（已知双卡组负收益，仅保留复验）
//! 未声明字段落回默认；未知 token 直接报错，防止实验漂移。

use anyhow::{anyhow, Result};
use rand::prelude::StdRng;

use crate::{
    game::{
        ramen::{RamenAction, RamenGame},
        Trainer
    },
    gamedata::{EventChoice, EventData},
    trainer::local_ramen_trainer::RecommendedRamenTrainer as PresetRamenTrainer
};

/// 从正式 preset 复制、按 `RAMEN_VARIANT` 覆盖少量参数的训练器。
pub struct IterationRamenTrainer {
    inner: PresetRamenTrainer,
    /// 解析出的变体标签（供日志与测试断言）。
    pub variant: String
}

impl IterationRamenTrainer {
    /// 默认：锁定冠军配置；存在 `RAMEN_VARIANT` 时按 token 覆盖。
    pub fn new() -> Self {
        let variant = std::env::var("RAMEN_VARIANT").unwrap_or_default();
        match Self::from_variant(&variant) {
            Ok(t) => t,
            Err(e) => panic!("RAMEN_VARIANT 无效: {e}")
        }
    }

    /// 按 token 串构造；空串即默认（gap75-ov100-sac260）冠军版。
    pub fn from_variant(variant: &str) -> Result<Self> {
        // 默认＝上游 preset ＋ 已验证增益（见模块注释决策日志）。
        let mut pt_rates = [16.0, 64.0, 64.0];
        let mut gap_strength = 0.75;
        let mut overflow_strength = 1.00;
        // 牺牲上限：sac 阶梯 260 处达峰（run 33065858798 / 33067050082），
        // 上游 preset 的 140 明显保守。
        let mut max_sacrifice = 260.0;
        let mut window_weight = 0.10;
        let reserve_max = 40.0;
        let early_bond = 8.0;
        let hint_bonus = 6.0;
        let mut region_weak_cover_weight = 0.0;

        for token in variant.split('-').filter(|t| !t.is_empty()) {
            if token == "pt32" {
                pt_rates = [32.0, 32.0, 32.0];
            } else if let Some(pct) = token.strip_prefix("gap") {
                gap_strength = Self::parse_percent(token, pct)?;
            } else if let Some(pct) = token.strip_prefix("ov") {
                overflow_strength = Self::parse_percent(token, pct)?;
            } else if let Some(per_mille) = token.strip_prefix("win") {
                window_weight = Self::parse_per_mille(token, per_mille)?;
            } else if let Some(raw) = token.strip_prefix("sac") {
                max_sacrifice = raw.parse().map_err(|_| anyhow!("token {token} 数值段非法: {raw}"))?;
            } else if let Some(raw) = token.strip_prefix("rwc") {
                region_weak_cover_weight =
                    raw.parse().map_err(|_| anyhow!("token {token} 数值段非法: {raw}"))?;
            } else {
                return Err(anyhow!("未知 RAMEN_VARIANT token: {token}"));
            }
        }

        Ok(Self {
            inner: PresetRamenTrainer::with_experiment_overrides(
                pt_rates,
                gap_strength,
                overflow_strength,
                max_sacrifice,
                window_weight,
                reserve_max,
                early_bond,
                hint_bonus,
                0.0, // 弱位 boost：走上游查找表（0.0＝自适应），对任意卡组与 preset 等价
                region_weak_cover_weight,
                true // 选面覆盖位预演硬门：回退根因的修复项，永久保持开启
            ),
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
    }

    #[test]
    fn locked_recipes_parse_cleanly() {
        // 两个卡组的最终配方必须都能干净解析（绝对值覆盖，顺序无关）。
        assert!(IterationRamenTrainer::from_variant("").is_ok());
        assert!(IterationRamenTrainer::from_variant("sac260").is_ok());
        assert!(IterationRamenTrainer::from_variant("sac230-win200").is_ok());
        assert_eq!(
            IterationRamenTrainer::from_variant("sac230-win200")
                .unwrap()
                .variant,
            "sac230-win200"
        );
    }
}
