//! Restored preset constructor used by the MCTS iteration workspace.
//!
//! 背景：迭代分支上一轮“只保留吃后必训一道硬门”的重构把选面阶段的
//! `eat_requires_covered_train` 预演门关闭，配对指标三项全面回退
//! （总分 -727.5 / 属性评分 -312.7 / 技能PT -194.4，见
//! `benchmark-results` 与 run 33048099445）。历史消融也早已证明：
//! 放开吃后动作自由度是大幅正向收益（v41：+8146 总分），真正被验证过
//! 的硬门是**选面前的覆盖位预演**。
//!
//! [`RecommendedRamenTrainer`] 内部的逐年构造不可从外部修改，因此这里用
//! 现成的 `with_experiment_overrides` 出口重建同一 preset，仅恢复该硬门；
//! 其余参数逐项等于 `new()` 的数值。weakboost 取 `-1.0` 显式关闭弱位
//! boost（本工作区卡组智卡=2，查找表结果同为 0.0，行为一致）。

use anyhow::Result;
use rand::prelude::StdRng;

use crate::{
    gamedata::{EventChoice, EventData},
    trainer::local_ramen_trainer::RecommendedRamenTrainer as PresetRamenTrainer,
    game::ramen::{RamenAction, RamenGame},
    game::Trainer
};

/// 与正式 preset 行为一致、但保留选面覆盖位预演门的训练器别名实现。
pub struct RestoredRamenTrainer {
    inner: PresetRamenTrainer
}

impl RestoredRamenTrainer {
    /// 从正式 preset 复制，仅恢复 `eat_requires_covered_train = true`。
    pub fn new() -> Self {
        Self {
            inner: PresetRamenTrainer::with_experiment_overrides(
                [16.0, 64.0, 64.0], // 分年 PT 权重（与 new() 一致）
                0.5, // status_gap_strength
                0.5, // status_overflow_strength
                140.0, // max_base_score_sacrifice
                0.10, // ramen_window_weight
                40.0, // status_reserve_max
                8.0, // early_bond_value
                6.0, // hint_bonus
                -1.0, // weakboost：显式关闭，等价于默认查找表在本卡组下的 0.0
                0.0, // region_weak_cover_weight（保持 policy 默认）
                true // ★ 恢复选面覆盖位预演硬门
            )
        }
    }

    /// 兼容矩阵工具的直接构造入口；覆盖语义原样透传。
    #[allow(clippy::too_many_arguments)]
    pub fn with_experiment_overrides(
        pt_rates: [f32; 3],
        gap_strength: f32,
        overflow_strength: f32,
        max_base_score_sacrifice: f32,
        ramen_window_weight: f32,
        status_reserve_max: f32,
        early_bond_value: f32,
        hint_bonus: f32,
        weakboost: f32,
        region_weak_cover_weight: f32,
        eat_requires_covered_train: bool
    ) -> Self {
        Self {
            inner: PresetRamenTrainer::with_experiment_overrides(
                pt_rates,
                gap_strength,
                overflow_strength,
                max_base_score_sacrifice,
                ramen_window_weight,
                status_reserve_max,
                early_bond_value,
                hint_bonus,
                weakboost,
                region_weak_cover_weight,
                eat_requires_covered_train
            )
        }
    }
}

impl Default for RestoredRamenTrainer {
    fn default() -> Self {
        Self::new()
    }
}

impl Trainer<RamenGame> for RestoredRamenTrainer {
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
