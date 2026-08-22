//! 本地拉面手写修正策略。
//!
//! 基础层完整复用上游 [`RamenPolicy`]；本层只补充基础策略没有显式估值的长期收益。
//! 根据首轮 1000 组配对测试，修正项采用保守估值，避免重复计算基础训练收益：
//! - 删除“训练人数越多固定加分”（基础收益已经包含支援效果，重复加人头分会误选低收益训练）；
//! - 羁绊价值随育成阶段衰减，后期不再为很难回本的羁绊牺牲当回合收益；
//! - 友人首次点击保留明显优先级，但不再使用足以压过优质训练的过大常数；
//! - 高失败率惩罚从 15% 后线性增加；
//! - 材料溢出只奖励及时吃面，不再同时惩罚“不吃”，避免把同一信号计算两次；
//! - 接近年度 RMJ 门槛时提高吃面优先级。

use std::sync::Mutex;

use anyhow::Result;
use rand::prelude::StdRng;

use crate::{
    game::{
        FriendOutState, Game, Person, PersonType, Trainer,
        ramen::{Operation, RamenAction, RamenGame, RamenStage, policy::{RamenPolicy, RamenPolicyOutput}}
    },
    gamedata::{EventChoice, EventData}
};

/// 本地修正项参数。分值与 `RamenPolicyOutput::score` 使用同一决策尺度。
#[derive(Debug, Clone)]
pub struct LocalRamenConfig {
    /// 一点有效羁绊在育成前期的价值。
    pub early_bond_value: f32,
    /// 有 Hint 的支援额外价值（羁绊增量已另算）。
    pub hint_bonus: f32,
    /// 友人尚未点击时，首次点击机会的总价值。
    pub first_friend_click_value: f32,
    /// 已点击但友人羁绊不足 60 时的价值。
    pub low_friend_bond_value: f32,
    /// 友人已启动且羁绊充足时的小额同行价值。
    pub active_friend_value: f32,
    /// 失败率超过阈值后，失败率达到 100% 时的最大附加惩罚。
    pub high_fail_penalty: f32,
    /// 风味库存超过此值才视为有溢出压力。
    pub feeling_overflow_threshold: i32,
    /// 每一点超阈值库存给“吃面”动作的奖励。
    pub overflow_value: f32,
    /// 距年度 RMJ 门槛不超过此值时进入冲线区。
    pub rmj_urgency_margin: i32,
    /// 恰好贴近门槛时给“吃面”动作的最大冲线奖励。
    pub rmj_urgency_bonus: f32
}

impl Default for LocalRamenConfig {
    fn default() -> Self {
        Self {
            early_bond_value: 8.0,
            hint_bonus: 6.0,
            first_friend_click_value: 90.0,
            low_friend_bond_value: 35.0,
            active_friend_value: 8.0,
            high_fail_penalty: 700.0,
            feeling_overflow_threshold: 10,
            overflow_value: 8.0,
            rmj_urgency_margin: 450,
            rmj_urgency_bonus: 60.0
        }
    }
}

/// B 策略：上游手写基准 `RamenPolicy` + 本地长期收益修正。
pub struct LocalRamenTrainer {
    pub policy: RamenPolicy,
    pub config: LocalRamenConfig,
    last_breakdown: Mutex<Option<String>>
}

impl Default for LocalRamenTrainer {
    fn default() -> Self {
        Self {
            policy: RamenPolicy::default(),
            config: LocalRamenConfig::default(),
            last_breakdown: Mutex::new(None)
        }
    }
}

impl LocalRamenTrainer {
    pub fn new() -> Self {
        Self::default()
    }

    fn choose(outputs: &[RamenPolicyOutput]) -> usize {
        outputs
            .iter()
            .enumerate()
            .max_by(|(ia, a), (ib, b)| a.score.total_cmp(&b.score).then_with(|| ib.cmp(ia)))
            .map(|(i, _)| i)
            .unwrap_or(0)
    }

    fn stash(&self, outputs: &[RamenPolicyOutput]) {
        let text = outputs
            .iter()
            .enumerate()
            .map(|(i, out)| format!("#{i} {:.0}[{}]", out.score, out.reason))
            .collect::<Vec<_>>()
            .join(" | ");
        if let Ok(mut slot) = self.last_breakdown.lock() {
            *slot = Some(text);
        }
    }

    /// 羁绊的回本时间随回合减少：前 24 回合全额、第二年 55%、第三年以后 15%。
    fn bond_phase_scale(turn: i32) -> f32 {
        if turn < 24 { 1.0 } else if turn < 48 { 0.55 } else { 0.15 }
    }

    fn decide_train(&self, game: &RamenGame, actions: &[RamenAction]) -> Result<(usize, Vec<RamenPolicyOutput>)> {
        let (guard_idx, mut outputs) = self.policy.decide_train(game, actions)?;
        if outputs.len() != actions.len() {
            return Ok((guard_idx, outputs));
        }

        let phase_scale = Self::bond_phase_scale(game.turn());
        for (action, out) in actions.iter().zip(outputs.iter_mut()) {
            let Operation::Train(train_type) = action.operation else { continue };
            let train = train_type as usize;
            let indices = game
                .distribution()
                .get(train)
                .into_iter()
                .flatten()
                .copied()
                .filter(|&p| p >= 0 && (p as usize) < game.persons().len())
                .map(|p| p as usize)
                .collect::<Vec<_>>();

            // 不再按总人头加分：训练即时收益已由上游 policy 计算，人头固定分属于重复估值。
            let mut long_term_score = 0.0;
            for &i in &indices {
                let person = &game.persons()[i];
                match person.person_type() {
                    PersonType::ScenarioCard => {
                        long_term_score += match game.friend.out_state {
                            FriendOutState::UnClicked => self.config.first_friend_click_value,
                            _ if person.friendship() < 60 => self.config.low_friend_bond_value * phase_scale,
                            _ => self.config.active_friend_value
                        };
                    }
                    PersonType::Card if person.friendship() < 80 => {
                        let mut gain: f32 = if game.uma.flags.aijiao { 9.0 } else { 7.0 };
                        if person.hint() {
                            gain += 5.0;
                        }
                        // 只计算真正能落到 80 羁绊线以内的有效增量。
                        gain = gain.min((80 - person.friendship()) as f32);
                        long_term_score += gain * self.config.early_bond_value * phase_scale;
                        if person.hint() {
                            long_term_score += self.config.hint_bonus;
                        }
                    }
                    PersonType::Card if person.hint() => {
                        long_term_score += self.config.hint_bonus;
                    }
                    _ => {}
                }
            }
            out.score += long_term_score;
            out.add("local_long_term_bond_friend_hint", long_term_score);

            // 15% 以下交给上游期望收益处理；只对高风险尾部追加非线性守门。
            let buffs = game.calc_training_buff(train)?;
            let fail_rate = game.calc_training_failure_rate(&buffs, train);
            if fail_rate > 15.0 {
                let excess_ratio = ((fail_rate - 15.0) / 85.0).clamp(0.0, 1.0);
                let penalty = -excess_ratio * self.config.high_fail_penalty;
                out.score += penalty;
                out.add("local_high_fail_tail", penalty);
            }
        }
        Ok((Self::choose(&outputs), outputs))
    }

    fn decide_ramen(&self, game: &RamenGame, actions: &[RamenAction]) -> Result<(usize, Vec<RamenPolicyOutput>)> {
        let (_, mut outputs) = self.policy.decide_ramen(game, actions)?;
        let stock_total: i32 = game.ramen.feeling_stock.iter().sum();
        let overflow = (stock_total - self.config.feeling_overflow_threshold).max(0) as f32;
        let year_idx = (game.current_year() - 1).clamp(0, 2) as usize;
        let target = [1500, 3000, 3500][year_idx];
        let gap = target - game.ramen.scenario_pt;

        for (action, out) in actions.iter().zip(outputs.iter_mut()) {
            if action.ramen.is_none() {
                continue;
            }

            // 只奖励吃面一次；不再对“不吃面”施加镜像惩罚，避免信号翻倍。
            let overflow_bonus = overflow * self.config.overflow_value;
            out.score += overflow_bonus;
            out.add("local_stock_overflow", overflow_bonus);

            if gap > 0 && gap <= self.config.rmj_urgency_margin {
                let closeness = 1.0 - gap as f32 / self.config.rmj_urgency_margin as f32;
                let urgency = self.config.rmj_urgency_bonus * (0.5 + 0.5 * closeness);
                out.score += urgency;
                out.add("local_rmj_urgency", urgency);
            }
        }
        Ok((Self::choose(&outputs), outputs))
    }
}

impl Trainer<RamenGame> for LocalRamenTrainer {
    fn select_action(&self, game: &RamenGame, actions: &[RamenAction], _rng: &mut StdRng) -> Result<usize> {
        if actions.len() <= 1 {
            return Ok(0);
        }
        let (idx, outputs) = match game.stage {
            RamenStage::Train => self.decide_train(game, actions)?,
            RamenStage::RamenSelect => self.decide_ramen(game, actions)?,
            RamenStage::SpecialSelect => self.policy.decide_special(game, actions)?,
            RamenStage::RegionSelect => {
                let year_idx = match game.turn() {
                    2 => 0,
                    23 => 1,
                    47 => 2,
                    _ => 0
                };
                self.policy.decide_region(game, year_idx, actions)?
            }
            _ => (0, Vec::new())
        };
        self.stash(&outputs);
        Ok(idx)
    }

    fn select_choice(&self, game: &RamenGame, choices: &[Vec<EventChoice>], _rng: &mut StdRng) -> Result<usize> {
        let (idx, outputs) = self.policy.decide_event(game, choices)?;
        self.stash(&outputs);
        Ok(idx)
    }

    fn select_event_choice(
        &self, game: &RamenGame, _event: &EventData, choices: &[Vec<EventChoice>], rng: &mut StdRng
    ) -> Result<usize> {
        self.select_choice(game, choices, rng)
    }

    fn last_breakdown(&self) -> Option<String> {
        self.last_breakdown.lock().ok().and_then(|slot| slot.clone())
    }
}
