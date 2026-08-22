//! 从 `uma-juece-ramen/rust/src/ramen_strategy.rs` 语义移植的拉面策略。
//!
//! 不覆盖上游 `RamenPolicy`：先复用其真实收益公式、自选比赛守门和事件期望，
//! 再叠加本地策略中有价值的羁绊/人头/友人点击、失败风险、材料溢出和 RMJ 紧迫度。
//! 这样可用同一模拟器、同一随机种子与上游 `RamenHandwrittenTrainer` 做公平 A/B。

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

#[derive(Debug, Clone)]
pub struct LocalRamenConfig {
    pub head_weight: f32,
    pub jiban_value: f32,
    pub hint_bonus: f32,
    pub friend_click_bonus: f32,
    pub big_fail_penalty: f32,
    pub feeling_overflow_threshold: i32,
    pub overflow_value: f32,
    pub rmj_urgency_margin: i32,
    pub rmj_urgency_bonus: f32
}

impl Default for LocalRamenConfig {
    fn default() -> Self {
        Self {
            head_weight: 15.0,
            jiban_value: 12.0,
            hint_bonus: 8.0,
            friend_click_bonus: 25.0,
            big_fail_penalty: 500.0,
            feeling_overflow_threshold: 8,
            overflow_value: 20.0,
            rmj_urgency_margin: 300,
            rmj_urgency_bonus: 40.0
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
            .map(|(i, out)| format!("#{i} {:.0}[{}]", out.score, out.reason))
            .collect::<Vec<_>>()
            .join(" | ");
        if let Ok(mut slot) = self.last_breakdown.lock() {
            *slot = Some(text);
        }
    }

    fn decide_train(&self, game: &RamenGame, actions: &[RamenAction]) -> Result<(usize, Vec<RamenPolicyOutput>)> {
        let (guard_idx, mut outputs) = self.policy.decide_train(game, actions)?;
        if outputs.len() != actions.len() {
            return Ok((guard_idx, outputs));
        }

        for (action, out) in actions.iter().zip(outputs.iter_mut()) {
            let Operation::Train(train_type) = action.operation else { continue };
            let train = train_type as usize;
            let indices = game.distribution().get(train).into_iter().flatten().copied()
                .filter(|&p| p >= 0 && (p as usize) < game.persons().len())
                .map(|p| p as usize)
                .collect::<Vec<_>>();

            let heads = indices.iter().filter(|&&i| {
                !matches!(game.persons()[i].person_type(), PersonType::Reporter | PersonType::Yayoi)
            }).count() as f32;
            let head_score = heads * self.config.head_weight;
            out.score += head_score;
            out.add("local_heads", head_score);

            let mut bond_score = 0.0;
            let mut has_unclicked_friend = false;
            for &i in &indices {
                let person = &game.persons()[i];
                match person.person_type() {
                    PersonType::ScenarioCard => {
                        has_unclicked_friend |= game.friend.out_state == FriendOutState::UnClicked;
                        bond_score += if game.friend.out_state == FriendOutState::UnClicked {
                            150.0
                        } else if person.friendship() < 60 {
                            100.0
                        } else {
                            40.0
                        };
                    }
                    PersonType::Card if person.friendship() < 80 => {
                        let mut gain: f32 = if game.uma.flags.aijiao { 9.0 } else { 7.0 };
                        if person.hint() { gain += 5.0; }
                        gain = gain.min((80 - person.friendship()) as f32);
                        bond_score += gain * self.config.jiban_value;
                        if person.hint() { bond_score += self.config.hint_bonus; }
                    }
                    PersonType::Card if person.hint() => bond_score += self.config.hint_bonus,
                    _ => {}
                }
            }
            if has_unclicked_friend {
                bond_score += self.config.friend_click_bonus;
            }
            out.score += bond_score;
            out.add("local_bond_friend_hint", bond_score);

            let buffs = game.calc_training_buff(train)?;
            let fail_rate = game.calc_training_failure_rate(&buffs, train);
            if fail_rate > 20.0 {
                let tail = -(fail_rate / 100.0) * self.config.big_fail_penalty;
                out.score += tail;
                out.add("local_big_fail_tail", tail);
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
            if action.ramen.is_some() {
                let overflow_bonus = overflow * self.config.overflow_value;
                out.score += overflow_bonus;
                out.add("local_stock_overflow", overflow_bonus);
                if gap > 0 && gap <= self.config.rmj_urgency_margin {
                    out.score += self.config.rmj_urgency_bonus;
                    out.add("local_rmj_urgency", self.config.rmj_urgency_bonus);
                }
            } else {
                let penalty = -overflow * self.config.overflow_value;
                out.score += penalty;
                out.add("local_stock_overflow", penalty);
            }
        }
        Ok((Self::choose(&outputs), outputs))
    }
}

impl Trainer<RamenGame> for LocalRamenTrainer {
    fn select_action(&self, game: &RamenGame, actions: &[RamenAction], _rng: &mut StdRng) -> Result<usize> {
        if actions.len() <= 1 { return Ok(0); }
        let (idx, outputs) = match game.stage {
            RamenStage::Train => self.decide_train(game, actions)?,
            RamenStage::RamenSelect => self.decide_ramen(game, actions)?,
            RamenStage::SpecialSelect => self.policy.decide_special(game, actions)?,
            RamenStage::RegionSelect => {
                let year_idx = match game.turn() { 2 => 0, 23 => 1, 47 => 2, _ => 0 };
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

    fn select_event_choice(&self, game: &RamenGame, _event: &EventData, choices: &[Vec<EventChoice>], rng: &mut StdRng) -> Result<usize> {
        self.select_choice(game, choices, rng)
    }

    fn last_breakdown(&self) -> Option<String> {
        self.last_breakdown.lock().ok().and_then(|slot| slot.clone())
    }
}
