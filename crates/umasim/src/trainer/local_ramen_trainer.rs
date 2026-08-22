//! B 策略：上游 `RamenPolicy` 基准 + 保守的前中期长期收益修正。
//!
//! 第一轮 1000 组配对结果：B 平均 +152，但差值中位数为 0，且存在较多大胜/大败。
//! 因此本轮目标不是继续放大均值，而是降低错误反选造成的尾部风险：
//! - RMJ 三年成功率已是 100%，吃面决策完全交还上游，不再额外干预；
//! - 羁绊修正只在前中期生效，第三年完全回退上游；
//! - 允许牺牲的上游基础训练分按阶段收紧；
//! - 高失败率已经被上游按期望值扣分，本地只对 30% 以上尾部做较小保护。

use std::sync::Mutex;

use anyhow::Result;
use rand::prelude::StdRng;

use crate::{
    game::{
        FriendOutState, Game, Person, PersonType, Trainer,
        ramen::{
            Operation, RamenAction, RamenGame, RamenStage,
            policy::{RamenPolicy, RamenPolicyOutput}
        }
    },
    gamedata::{EventChoice, EventData}
};

#[derive(Debug, Clone)]
pub struct LocalRamenConfig {
    /// 第一年每点有效羁绊价值。
    pub early_bond_value: f32,
    /// Hint 的独立附加价值。
    pub hint_bonus: f32,
    /// 友人尚未首次点击时的机会价值。
    pub first_friend_click_value: f32,
    /// 友人已启动但羁绊不足 60 的价值。
    pub low_friend_bond_value: f32,
    /// 30% 以上失败率尾部的最大附加惩罚。
    pub high_fail_penalty: f32,
    /// 第一年最多允许牺牲的上游基础训练分。
    pub early_max_sacrifice: f32,
    /// 第二年最多允许牺牲的上游基础训练分。
    pub middle_max_sacrifice: f32
}

impl Default for LocalRamenConfig {
    fn default() -> Self {
        Self {
            early_bond_value: 7.0,
            hint_bonus: 5.0,
            first_friend_click_value: 75.0,
            low_friend_bond_value: 25.0,
            high_fail_penalty: 300.0,
            early_max_sacrifice: 80.0,
            middle_max_sacrifice: 40.0
        }
    }
}

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
            .map(|(i, output)| format!("#{i} {:.0}[{}]", output.score, output.reason))
            .collect::<Vec<_>>()
            .join(" | ");
        if let Ok(mut slot) = self.last_breakdown.lock() {
            *slot = Some(text);
        }
    }

    /// 返回 `(长期价值倍率, 最大基础分牺牲)`。
    /// 第三年不追羁绊，也不允许本地长期项反选上游训练。
    fn phase_limits(&self, turn: i32) -> (f32, f32) {
        if turn < 24 {
            (1.0, self.config.early_max_sacrifice)
        } else if turn < 48 {
            (0.35, self.config.middle_max_sacrifice)
        } else {
            (0.0, 0.0)
        }
    }

    fn decide_train(
        &self, game: &RamenGame, actions: &[RamenAction]
    ) -> Result<(usize, Vec<RamenPolicyOutput>)> {
        let (guard_idx, mut outputs) = self.policy.decide_train(game, actions)?;
        if outputs.len() != actions.len() {
            return Ok((guard_idx, outputs));
        }

        let base_scores = outputs.iter().map(|output| output.score).collect::<Vec<_>>();
        let base_best = Self::choose(&outputs);
        let (phase_scale, max_sacrifice) = self.phase_limits(game.turn());

        for (action, output) in actions.iter().zip(outputs.iter_mut()) {
            let Operation::Train(train_type) = action.operation else {
                continue;
            };
            let train = train_type as usize;
            let people = game
                .distribution()
                .get(train)
                .into_iter()
                .flatten()
                .copied()
                .filter(|&person| person >= 0 && (person as usize) < game.persons().len())
                .map(|person| person as usize);

            let mut long_term = 0.0;
            if phase_scale > 0.0 {
                for index in people {
                    let person = &game.persons()[index];
                    match person.person_type() {
                        PersonType::ScenarioCard => {
                            long_term += match game.friend.out_state {
                                FriendOutState::UnClicked => self.config.first_friend_click_value * phase_scale,
                                _ if person.friendship() < 60 => self.config.low_friend_bond_value * phase_scale,
                                // 已启动且羁绊足够：即时训练收益已由上游计算，不再固定加分。
                                _ => 0.0
                            };
                        }
                        PersonType::Card if person.friendship() < 80 => {
                            let mut gain: f32 = if game.uma.flags.aijiao { 9.0 } else { 7.0 };
                            if person.hint() {
                                gain += 5.0;
                            }
                            gain = gain.min((80 - person.friendship()) as f32);
                            long_term += gain * self.config.early_bond_value * phase_scale;
                            if person.hint() {
                                long_term += self.config.hint_bonus * phase_scale;
                            }
                        }
                        PersonType::Card if person.hint() => {
                            long_term += self.config.hint_bonus * phase_scale;
                        }
                        _ => {}
                    }
                }
            }
            output.score += long_term;
            output.add("local_long_term", long_term);

            // 上游已经扣除失败率期望损失。本地仅防 30% 以上的高风险尾部，避免重复重罚。
            let buffs = game.calc_training_buff(train)?;
            let fail_rate = game.calc_training_failure_rate(&buffs, train);
            if fail_rate > 30.0 {
                let tail_ratio = ((fail_rate - 30.0) / 70.0).clamp(0.0, 1.0);
                let penalty = -tail_ratio * self.config.high_fail_penalty;
                output.score += penalty;
                output.add("local_high_fail_tail", penalty);
            }
        }

        let local_best = Self::choose(&outputs);
        let sacrifice = base_scores[base_best] - base_scores[local_best];
        let chosen = if sacrifice <= max_sacrifice {
            local_best
        } else {
            base_best
        };
        if sacrifice > max_sacrifice {
            outputs[chosen].reason.push_str(&format!(
                ";保护: 本地候选牺牲基础分{sacrifice:.0}>阶段上限{max_sacrifice:.0}"
            ));
        }
        Ok((chosen, outputs))
    }
}

impl Trainer<RamenGame> for LocalRamenTrainer {
    fn select_action(
        &self, game: &RamenGame, actions: &[RamenAction], _rng: &mut StdRng
    ) -> Result<usize> {
        if actions.len() <= 1 {
            return Ok(0);
        }
        let (idx, outputs) = match game.stage {
            RamenStage::Train => self.decide_train(game, actions)?,
            // 第一轮 A/B 中两者 RMJ 均为 3.00/3 年；吃面不再加入本地启发式。
            RamenStage::RamenSelect => self.policy.decide_ramen(game, actions)?,
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

    fn select_choice(
        &self, game: &RamenGame, choices: &[Vec<EventChoice>], _rng: &mut StdRng
    ) -> Result<usize> {
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
