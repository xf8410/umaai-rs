//! 配对指标迭代用训练器包装。
//!
//! 背景：上游 a6e1f48 的正式 preset 含选面阶段 `eat_requires_covered_train`
//! 预演硬门；本工作区某轮重构把它关闭后三项配对指标全面回退，恢复后与基线
//! 300 局逐种子完全同轨（各项变化恒为 +0.000）。本模块因此承担两个职责：
//!
//! 1. 默认（无环境变量）重建“基线等价、覆盖位门开启”的配置；
//! 2. 通过环境变量 `RAMEN_VARIANT` 叠加历史矩阵验证过的参数变体，
//!    用于坐标式邻域扫描。token 以 `-` 连接：
//!    - `pt32`   分年技能 PT 权重 32/32/32（复赛在 2速2耐1力 上总分 +1036）
//!    - `gap75`  短板追赶强度 0.75
//!    - `ov100`  近上限衰减强度 1.00
//!    未声明字段落回默认值；未知 token 触发 panic，防止实验漂移。

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
///
/// `new()` 保持与原 [`PresetRamenTrainer`] 相同的返回类型（`Self`），
/// 这样既有二进制无需改动即可编译；变体非法时直接 panic 暴露配置错误。
pub struct IterationRamenTrainer {
    inner: PresetRamenTrainer,
    /// 解析出的变体标签（供日志与测试断言）。
    pub variant: String
}

impl IterationRamenTrainer {
    /// 默认：基线等价配置；存在 `RAMEN_VARIANT` 时按 token 覆盖。
    pub fn new() -> Self {
        let variant = std::env::var("RAMEN_VARIANT").unwrap_or_default();
        match Self::from_variant(&variant) {
            Ok(t) => t,
            Err(e) => panic!("RAMEN_VARIANT 无效: {e}")
        }
    }

    /// 按 token 串构造；空串即纯恢复版。
    pub fn from_variant(variant: &str) -> Result<Self> {
        // 基线：与正式 preset 相同的已固化数值。
        let mut pt_rates = [16.0, 64.0, 64.0];
        let mut gap_strength = 0.5;
        let mut overflow_strength = 0.5;
        let max_sacrifice = 140.0;
        let window_weight = 0.10;
        let reserve_max = 40.0;
        let early_bond = 8.0;
        let hint_bonus = 6.0;

        for token in variant.split('-').filter(|t| !t.is_empty()) {
            match token {
                "pt32" => pt_rates = [32.0, 32.0, 32.0],
                "gap75" => gap_strength = 0.75,
                "ov100" => overflow_strength = 1.00,
                other => return Err(anyhow!("未知 RAMEN_VARIANT token: {other}"))
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
                true // ★ 选面覆盖位预演硬门：本轮修复的核心保留项
            ),
            variant: variant.to_string()
        })
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

/// 变体解析必须严格：未知 token 报错而不是静默回退。
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_variant_token_fails() {
        let err = match IterationRamenTrainer::from_variant("nonsense") {
            Err(e) => e,
            Ok(_) => panic!("未知 token 应报错")
        };
        assert!(err.to_string().contains("未知 RAMEN_VARIANT token"));
    }

    #[test]
    fn empty_variant_is_pure_restore() {
        let t = IterationRamenTrainer::from_variant("").expect("空变体应合法");
        assert_eq!(t.variant, "");
    }
}
