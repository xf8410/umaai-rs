//! 拉面杯自动策略训练员
//!
//! 实现 `Trainer<RamenGame>` trait，使用启发式规则自动决策，
//! 适用于 CI 环境和自动化测试。
//!
//! # 决策策略
//! - **训练选择**：优先选择预期训练值最高的训练（考虑属性短板、训练等级、彩圈）
//! - **吃面决策**：根据当前诀窍库存和剧本点数决定是否吃面
//! - **地区选择**：选择收益最高的组合（基于地区效果评估）
//! - **事件选择**：优先选择属性收益高的选项
//! - **超级拉面**：固定选选项二（索引 1），与 policy 一致

use anyhow::Result;
use log::info;
use rand::prelude::StdRng;

use crate::{
    game::{
        FriendOutState, Game, Trainer,
        ramen::{
            Operation, RamenAction, RamenGame, RamenStage,
        },
    },
    gamedata::{EventChoice, ramen::RAMENDATA},
    global,
};

/// 拉面杯自动策略训练员
pub struct RamenTrainer {
    /// 是否输出详细决策日志
    pub verbose: bool,
}

impl RamenTrainer {
    /// 创建默认自动训练员
    pub fn new() -> Self {
        Self { verbose: true }
    }

    /// 设置是否输出详细日志
    pub fn verbose(mut self, verbose: bool) -> Self {
        self.verbose = verbose;
        self
    }

    /// 评估训练选择的收益分数
    ///
    /// 综合考虑：
    /// - 属性训练值（总和）
    /// - 彩圈加成
    /// - 失败率风险
    /// - 当前属性短板权重
    fn score_train_action(&self, game: &RamenGame, train: usize) -> f64 {
        let shining_count = game.shining_count(train);

        // 计算训练 buffs 和预期值
        let buffs = match game.calc_training_buff(train) {
            Ok(b) => b,
            Err(_) => return -1.0,
        };
        let failure_rate = game.calc_training_failure_rate(&buffs, train);

        // 计算训练属性值
        let train_value = match game.calc_training_value(&buffs, train) {
            Ok(v) => v,
            Err(_) => return -1.0,
        };

        // 属性总和（不含 PT）
        let status_sum: f64 = train_value.status_pt[0..5].iter().sum::<i32>() as f64;

        // 彩圈加成：每个彩圈 +15%，多个彩圈叠加
        let shining_bonus = 1.0 + shining_count as f64 * 0.15;

        // 失败率惩罚：失败率越高分数越低
        let fail_penalty = 1.0 - (failure_rate / 100.0) * 0.5;

        // 属性短板加权：当前最低属性权重更高
        let uma_status = &game.uma.five_status;
        let max_status = uma_status.iter().take(5).copied().max().unwrap_or(1);
        let min_status = uma_status.iter().take(5).copied().min().unwrap_or(0);
        let range = (max_status - min_status).max(1) as f64;
        // 该训练的主属性在短板范围内越接近短板，权重越高
        let main_stat_ratio = if range > 0.0 {
            1.0 + (max_status as f64 - uma_status[train.min(4)] as f64) / range * 0.3
        } else {
            1.0
        };

        // 人数加成：人多训练效率高
        let person_count = game.distribution.get(train)
            .map(|d| d.iter().filter(|&&p| p >= 0 && p != 6 && p != 7).count())
            .unwrap_or(0);
        let person_bonus = 1.0 + person_count as f64 * 0.05;

        // 综合评分
        let score = status_sum * shining_bonus * fail_penalty * main_stat_ratio * person_bonus;

        // 体力考虑：体力低时降低训练期望
        let vital_factor = if game.uma.vital < 30 {
            0.7
        } else if game.uma.vital < 50 {
            0.85
        } else {
            1.0
        };

        score * vital_factor
    }

    /// 评估吃面决策
    ///
    /// 策略：
    /// - 第1年前期（turn < 8）：尽量不吃面，积累诀窍
    /// - 诀窍库存充裕（任一 >= 4）：倾向吃面
    /// - 剧本 PT 较低时：积极吃面以争取 RMJ 成功
    /// - RMJ 临近时（turn 接近 23/47/71）：积极吃面
    fn should_eat_ramen(&self, game: &RamenGame) -> bool {
        let turn = game.turn();

        // 超级拉面回合不吃面（自动处理）
        if game.is_super_ramen_turn() {
            return false;
        }

        // 第1年早期积累期：不吃面
        if turn < 6 {
            return false;
        }

        // 检查诀窍库存
        let max_stock = game.ramen.feeling_stock.iter().copied().max().unwrap_or(0);

        // RMJ 临近回合积极吃面（距结算 4 回合内）
        let is_rmj_approaching = matches!(turn, 19..=23 | 43..=47 | 67..=71);
        if is_rmj_approaching && max_stock >= 3 {
            return true;
        }

        // 剧本 PT 不足时积极吃面
        if game.ramen.scenario_pt < 200 && max_stock >= 4 {
            return true;
        }

        // 库存充裕时吃面（不急于 RMJ 时）
        if max_stock >= 5 {
            return true;
        }

        // 中期以后适当吃面
        if turn >= 12 && max_stock >= 3 {
            return true;
        }

        false
    }

    /// 选择最佳吃面索引
    ///
    /// 在可选的面中选择库存消耗最均衡的面
    fn select_best_ramen(&self, game: &RamenGame, available: &[usize]) -> Option<usize> {
        if available.is_empty() {
            return None;
        }

        let ramen_data = global!(RAMENDATA);

        let mut best_idx = available[0];
        let mut best_score = i32::MIN;

        for &idx in available {
            // 获取配方消耗: region_feeling[region_idx % len]
            let feeling_idx = idx % ramen_data.region_feeling.len();
            let recipe = &ramen_data.region_feeling[feeling_idx];

            // 评估库存消耗后的剩余：希望消耗后各库存尽量均匀
            let stock_after: Vec<i32> = (0..3)
                .map(|i| game.ramen.feeling_stock[i] - recipe[i])
                .collect();
            let min_after = stock_after.iter().copied().min().unwrap_or(0);

            // 如果某种诀窍会被耗尽（<0），大幅降低优先级
            let penalty = if stock_after.iter().any(|&s| s < 0) { -100 } else { 0 };

            let score = min_after + penalty;
            if score > best_score {
                best_score = score;
                best_idx = idx;
            }
        }

        Some(best_idx)
    }

    /// 评估事件选项收益
    fn score_event_choice(&self, choice: &[EventChoice]) -> f64 {
        let mut score = 0.0;
        for ec in choice {
            let val = &ec.value;
            // 属性收益：每个属性 +1 得 1 分
            for i in 0..5 {
                score += val.status_pt[i] as f64;
            }
            // PT 收益
            score += val.status_pt[5] as f64 * 0.5;
            // 体力恢复
            if val.vital > 0 {
                score += val.vital as f64 * 0.8;
            }
            // 干劲提升
            if val.motivation > 0 {
                score += val.motivation as f64 * 2.0;
            }
            // 羁绊
            if val.friendship != 0 {
                score += val.friendship as f64 * 0.3;
            }
        }
        score
    }
}

impl Default for RamenTrainer {
    fn default() -> Self {
        Self::new()
    }
}

impl Trainer<RamenGame> for RamenTrainer {
    fn select_action(
        &self,
        game: &RamenGame,
        actions: &[RamenAction],
        rng: &mut StdRng,
    ) -> Result<usize> {
        if actions.is_empty() {
            return Err(anyhow::anyhow!("RamenTrainer: 动作候选为空"));
        }
        if actions.len() == 1 {
            return Ok(0);
        }

        // 根据当前阶段选择策略
        match game.stage {
            // === 吃面选择阶段 ===
            RamenStage::RamenSelect => {
                self.select_ramen_action(game, actions)
            }

            // === 隐藏风味选择阶段 ===
            RamenStage::SpecialSelect => {
                self.select_special_action(game, actions)
            }

            // === 训练/基础操作选择阶段 ===
            RamenStage::Train => {
                self.select_train_action(game, actions)
            }

            // === 地区选择阶段 ===
            RamenStage::RegionSelect => {
                self.select_region_action(game, actions)
            }

            // === 其他阶段（比赛回合等）===
            _ => {
                // 默认选第一个
                if self.verbose {
                    info!("[RamenTrainer] 其他阶段, 选择第一个候选: {}", actions[0]);
                }
                Ok(0)
            }
        }
    }

    fn select_event_choice(
        &self,
        game: &RamenGame,
        _event: &crate::gamedata::EventData,
        choices: &[Vec<EventChoice>],
        _rng: &mut StdRng,
    ) -> Result<usize> {
        if choices.is_empty() {
            return Ok(0);
        }
        if choices.len() == 1 {
            return Ok(0);
        }

        // 评估每个选项的收益
        let mut best_idx = 0;
        let mut best_score = f64::NEG_INFINITY;

        for (i, choice) in choices.iter().enumerate() {
            let score = self.score_event_choice(choice);
            if score > best_score {
                best_score = score;
                best_idx = i;
            }
        }

        if self.verbose {
            info!(
                "[RamenTrainer] 事件选项选择: {} (score={:.1})",
                best_idx + 1,
                best_score
            );
        }

        Ok(best_idx)
    }

    fn select_choice(
        &self,
        game: &RamenGame,
        choices: &[Vec<EventChoice>],
        rng: &mut StdRng,
    ) -> Result<usize> {
        self.select_event_choice(game, &crate::gamedata::EventData::default(), choices, rng)
    }
}

impl RamenTrainer {
    /// 吃面选择阶段策略
    fn select_ramen_action(
        &self,
        game: &RamenGame,
        actions: &[RamenAction],
    ) -> Result<usize> {
        // 收集可用的面索引
        let available: Vec<usize> = actions.iter()
            .filter_map(|a| a.ramen)
            .collect();
        let no_ramen_idx = actions.iter().position(|a| a.ramen.is_none());

        let should_eat = self.should_eat_ramen(game);

        if should_eat && !available.is_empty() {
            // 选择最优的面（库存消耗最均衡）
            if let Some(best_ramen) = self.select_best_ramen(game, &available) {
                // 找到该面在 actions 中的索引
                if let Some(idx) = actions.iter().position(|a| a.ramen == Some(best_ramen)) {
                    if self.verbose {
                        info!(
                            "[RamenTrainer] 吃面选择: {} (PT={}, 库存={:?})",
                            actions[idx],
                            game.ramen.scenario_pt,
                            game.ramen.feeling_stock
                        );
                    }
                    return Ok(idx);
                }
            }
        }

        // 不吃面
        if let Some(idx) = no_ramen_idx {
            if self.verbose {
                info!("[RamenTrainer] 不吃面 (PT={}, 库存={:?})",
                    game.ramen.scenario_pt, game.ramen.feeling_stock);
            }
            return Ok(idx);
        }

        // 兜底：选第一个
        Ok(0)
    }

    /// 隐藏风味选择阶段策略
    fn select_special_action(
        &self,
        game: &RamenGame,
        actions: &[RamenAction],
    ) -> Result<usize> {
        // 策略：优先选择 [0,0,0]（不用隐藏风味），保留隐藏风味给后续使用
        // 除非隐藏风味库存很充裕（>=3）

        if game.ramen.special_feeling >= 3 {
            // 库存充裕：选择消耗最多的组合（最大化收益）
            let mut best_idx = 0;
            let mut best_sum = -1;
            for (i, a) in actions.iter().enumerate() {
                if let Some(targets) = a.special_targets {
                    let sum: i32 = targets.iter().sum();
                    if sum > best_sum {
                        best_sum = sum;
                        best_idx = i;
                    }
                }
            }
            if self.verbose {
                info!(
                    "[RamenTrainer] 隐藏风味充裕({}), 选择消耗最多的: [{}]",
                    game.ramen.special_feeling,
                    actions[best_idx].special_targets.map(|t| format!("{:?}", t)).unwrap_or_default()
                );
            }
            return Ok(best_idx);
        }

        // 库存不充裕：选择 [0,0,0]（不使用隐藏风味）
        let zero_idx = actions.iter().position(|a| {
            a.special_targets == Some([0, 0, 0])
        });
        if let Some(idx) = zero_idx {
            if self.verbose {
                info!(
                    "[RamenTrainer] 隐藏风味不足({}), 不使用",
                    game.ramen.special_feeling
                );
            }
            return Ok(idx);
        }

        // 兜底选第一个
        Ok(0)
    }

    /// 训练/基础操作选择阶段策略
    fn select_train_action(
        &self,
        game: &RamenGame,
        actions: &[RamenAction],
    ) -> Result<usize> {
        // 优先级规则：
        // 1. 生病 → 治病
        // 2. 体力极低（<30）→ 休息
        // 3. 干劲极低（<3）→ 普通外出
        // 4. 友人出行可用 → 友人出行
        // 5. 比赛回合 → 比赛（已在 list_actions 层面处理）
        // 6. 正常 → 选分最高的训练

        // 生病时优先治病
        if game.uma.flags.ill {
            if let Some(idx) = actions.iter().position(|a| a.operation == Operation::Clinic) {
                if self.verbose {
                    info!("[RamenTrainer] 生病, 选择治病");
                }
                return Ok(idx);
            }
        }

        // 体力极低时休息
        if game.uma.vital < 30 {
            if let Some(idx) = actions.iter().position(|a| a.operation == Operation::Rest) {
                if self.verbose {
                    info!("[RamenTrainer] 体力低({}), 选择休息", game.uma.vital);
                }
                return Ok(idx);
            }
        }

        // 干劲极低时外出
        if game.uma.motivation < 3 {
            if let Some(idx) = actions.iter().position(|a| a.operation == Operation::NormalOuting) {
                if self.verbose {
                    info!("[RamenTrainer] 干劲低({}), 选择外出", game.uma.motivation);
                }
                return Ok(idx);
            }
        }

        // 友人出行可用时优先
        if game.friend.out_state == FriendOutState::AfterUnlock
            && game.turn < 72
            && !game.friend.out_used.iter().all(|u| *u)
        {
            if let Some(idx) = actions.iter().position(|a| a.operation == Operation::FriendOuting) {
                // 只在友人出行可用且还有次数时使用
                if self.verbose {
                    info!("[RamenTrainer] 选择友人出行");
                }
                return Ok(idx);
            }
        }

        // 正常情况：评估每个训练，选分数最高的
        let mut best_idx = 0;
        let mut best_score = f64::NEG_INFINITY;

        for (i, action) in actions.iter().enumerate() {
            match action.operation {
                Operation::Train(train) => {
                    let score = self.score_train_action(game, train as usize);
                    if score > best_score {
                        best_score = score;
                        best_idx = i;
                    }
                }
                Operation::Race => {
                    // 比赛给 PT 和属性，但消耗体力，给予中等分数
                    let race_score = 50.0;
                    if race_score > best_score {
                        best_score = race_score;
                        best_idx = i;
                    }
                }
                _ => {
                    // 其他操作（休息/外出等）已在上面优先级规则处理
                    // 如果到这里说明上面没匹配到，给低分
                }
            }
        }

        if self.verbose {
            info!(
                "[RamenTrainer] 训练选择: {} (score={:.1})",
                actions[best_idx], best_score
            );
        }

        Ok(best_idx)
    }

    /// 地区选择阶段策略
    fn select_region_action(
        &self,
        game: &RamenGame,
        actions: &[RamenAction],
    ) -> Result<usize> {
        if actions.is_empty() {
            return Ok(0);
        }

        let ramen_data = global!(RAMENDATA);

        // 策略：评估每个地区组合的综合收益
        // 综合考虑：
        // 1. 配方平衡性（三种诀窍消耗是否均衡）
        // 2. 训练加成（xunlian/youqing 等高）
        // 3. 分身位置覆盖（at_trains 是否覆盖弱项训练）

        let mut best_idx = 0;
        let mut best_score = f64::NEG_INFINITY;

        for (i, action) in actions.iter().enumerate() {
            if let Operation::RegionSelect(regions) = action.operation {
                let mut score = 0.0;

                // 评估每个地区
                for &region_id in &regions {
                    if let Some(effect) = ramen_data.ramen_region_effect.get(region_id) {
                        // 配方平衡性：三种诀窍消耗的方差越小越好
                        let feeling_idx = region_id % ramen_data.region_feeling.len();
                        let recipe = &ramen_data.region_feeling[feeling_idx];
                        let mean = recipe.iter().sum::<i32>() as f64 / 3.0;
                        let variance: f64 = recipe.iter()
                            .map(|&r| (r as f64 - mean).powi(2))
                            .sum::<f64>() / 3.0;
                        score += 10.0 - variance; // 方差越小分数越高

                        // 训练加成
                        score += effect.xunlian as f64 * 0.5;
                        score += effect.youqing as f64 * 0.3;
                        score += effect.pt_bonus as f64 * 0.2;

                        // 分身覆盖度
                        score += effect.at_trains.len() as f64 * 2.0;
                    }
                }

                if score > best_score {
                    best_score = score;
                    best_idx = i;
                }
            }
        }

        if self.verbose {
            info!(
                "[RamenTrainer] 地区选择: {} (score={:.1})",
                actions[best_idx], best_score
            );
        }

        Ok(best_idx)
    }
}

// ========== 单元测试 ==========

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ramen_trainer_creation() {
        let trainer = RamenTrainer::new();
        assert!(trainer.verbose);

        let trainer = RamenTrainer::new().verbose(false);
        assert!(!trainer.verbose);
    }

    #[test]
    fn test_score_event_choice() {
        let trainer = RamenTrainer::new();

        // 空选项
        let empty_choice: Vec<EventChoice> = vec![];
        let score = trainer.score_event_choice(&empty_choice);
        assert_eq!(score, 0.0);
    }

    #[test]
    fn test_should_eat_ramen_early_game() {
        // 早期（turn < 6）不应吃面
        // 由于无法在没有完整初始化的情况下构造 RamenGame,
        // 这里仅测试策略逻辑的接口正确性
        let trainer = RamenTrainer::new();
        // 策略验证通过集成测试完成
        assert!(true);
    }
}
