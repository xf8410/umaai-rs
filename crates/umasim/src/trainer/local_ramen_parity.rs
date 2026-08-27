//! 配对指标迭代用训练器包装。
//!
//! 锁定记录（run 33050390060，配卡 2速1耐2智，基线 @ a6e1f48，300 局配对）：
//! 动态属性平衡坐标上移到 gap=0.75 / overflow=1.00 后三项全部严格提升
//! （总分 +40.587 / 属性评分 +35.040 / 技能PT +2.230），已固化为**默认配置**。
//! 同场对照证明 `pt32` 在本卡组为负收益（总分 -430.8），不采用。
//!
//! 因此语义从“恢复基线”升级为“基线 + 已验证增益”：
//!
//! 1. 无环境变量时直接使用锁定冠军参数；
//! 2. 环境变量 `RAMEN_VARIANT` 用带数值后缀的 token 做邻域收缩，
//!    全部是**绝对值覆盖**（如 `gap100` 即 1.00，与当前默认值无关），
//!    可自由组合：`gap100-ov075`。token 集：
//!    - `gapNNN`  短板追赶强度 NNN%（例：`gap100` = 1.00）
//!    - `ovNNN`   近上限衰减强度 NNN%（例：`ov075` = 0.75）
//!    - `pt32`    分年技能 PT 权重 32/32/32（本卡组已知负收益，仅保留作复验）
//!    未声明字段落回默认；未知 token 直接报错，防止实验漂移。

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

    /// 按 token 串构造；空串即锁定冠军版。
    pub fn from_variant(variant: &str) -> Result<Self> {
        // 默认＝上游 preset ＋ 已验证动态属性平衡坐标（见模块注释）。
        let mut pt_rates = [16.0, 64.0, 64.0];
        let mut gap_strength = 0.75;
        let mut overflow_strength = 1.00;
        let max_sacrifice = 140.0;
        let window_weight = 0.10;
        let reserve_max = 40.0;
        let early_bond = 8.0;
        let hint_bonus = 6.0;

        for token in variant.split('-').filter(|t| !t.is_empty()) {
            if token == "pt32" {
                pt_rates = [32.0, 32.0, 32.0];
            } else if let Some(pct) = token.strip_prefix("gap") {
                gap_strength = Self::parse_percent(token, pct)?;
            } else if let Some(pct) = token.strip_prefix("ov") {
                overflow_strength = Self::parse_percent(token, pct)?;
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
                -1.0, // 弱位 boost 显式关闭：本卡组智卡=2，查找表结果同为 0.0
                0.0,
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
}
