//! 拉面杯实验策略：在现有即时评分上增加长期训练结构与剧本 PT 阈值价值。
use crate::{
    game::{
        FriendOutState, Game, Person, PersonType, Trainer,
        ramen::{
            Operation, RamenAction, RamenGame, RamenStage,
            effects::calc_ramen_training_effect,
            policy::{RamenPolicy, RamenPolicyConfig, RamenPolicyOutput},
            rules::{calc_ramen_pt_gain, calc_region_bonus, consume_for_ramen, list_special_targets_for},
        },
    },
    gamedata::{EventChoice, EventData, ramen::RAMENDATA},
};
use anyhow::Result;
use rand::{SeedableRng, prelude::StdRng};
use std::sync::Mutex;

#[derive(Debug, Clone)]
pub struct LocalRamenConfig {
    pub early_bond_value: f32,
    pub hint_bonus: f32,
    pub first_friend_click_value: f32,
    pub low_friend_bond_value: f32,
    pub active_friend_value: f32,
    pub high_fail_penalty: f32,
    pub feeling_overflow_threshold: i32,
    pub overflow_value: f32,
    pub max_base_score_sacrifice: f32,
    pub status_reserve_max: f32,
    pub dynamic_vital: bool,
    pub probabilistic_hint: bool,
    pub expected_fail: bool,
    /// 吃面跨越 scenario_pt 常驻档位后，每个剩余回合的近似价值倍率。
    pub checkpoint_scale: f32,
    /// 本次吃面首次跨过当年 RMJ 成功线时的一次性价值。
    pub rmj_cross_bonus: f32,
    /// 第三年本次吃面首次跨过 5000 大成功线时的一次性价值。
    pub great_cross_bonus: f32,
    /// 吃面前前向评估中，“吃面后最佳实际动作”相对“不吃面最佳动作”的增量权重。
    pub ramen_lookahead_weight: f32,
    /// 地区分身存在随机性时的事前采样数；只作用于状态副本，不读取真实吃面结果。
    pub ramen_lookahead_samples: usize,
    /// 积极吃面节奏：存在可制作面时，前向值只负责在面之间排序，不与“不吃”竞争。
    pub eager_eat: bool,
    /// v8 诊断：吃面前，候选地区覆盖的当前训练窗口价值。
    pub ramen_window_weight: f32,
    /// Match the canonical policy switch for the local expected-failure layer.
    pub effective_ramen_failure: bool,
    /// First-year-only safety bridge: minimum raw failure rate of the rescued training.
    pub safety_bridge_min_fail: f32,
    /// Minimum score improvement required after applying Y1's 30% relative failure reduction.
    pub safety_bridge_min_gain: f32,
    /// Cost per lost post-eat craftable option and per hidden flavor consumed.
    pub safety_bridge_stock_cost: f32,
}
impl Default for LocalRamenConfig {
    fn default() -> Self {
        Self {
            early_bond_value: 8.,
            hint_bonus: 6.,
            first_friend_click_value: 75.,
            low_friend_bond_value: 35.,
            active_friend_value: 8.,
            high_fail_penalty: 0.,
            feeling_overflow_threshold: 8,
            overflow_value: 8.,
            max_base_score_sacrifice: 140.,
            status_reserve_max: 0.,
            dynamic_vital: false,
            probabilistic_hint: false,
            expected_fail: false,
            checkpoint_scale: 0.,
            rmj_cross_bonus: 0.,
            great_cross_bonus: 0.,
            ramen_lookahead_weight: 1.0,
            ramen_lookahead_samples: 12,
            eager_eat: false,
            ramen_window_weight: 0.0,
            effective_ramen_failure: true,
            safety_bridge_min_fail: 101.0,
            safety_bridge_min_gain: 0.0,
            safety_bridge_stock_cost: 0.0,
        }
    }
}
pub struct LocalRamenTrainer {
    policy: RamenPolicy,
    config: LocalRamenConfig,
    last_breakdown: Mutex<Option<String>>,
}
impl Default for LocalRamenTrainer {
    fn default() -> Self {
        Self::with_configs(RamenPolicyConfig::default(), LocalRamenConfig::default())
    }
}
impl LocalRamenTrainer {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn with_configs(policy: RamenPolicyConfig, config: LocalRamenConfig) -> Self {
        Self {
            policy: RamenPolicy::new(policy),
            config,
            last_breakdown: Mutex::new(None),
        }
    }
    pub fn matrix_variant(name: &str) -> Result<Self> {
        let mut policy = RamenPolicyConfig::default();
        let mut local = LocalRamenConfig::default();
        let (mut p, mut s, mut m, mut f) = (false, false, false, false);
        for token in name.split('-') {
            if token == "rawfail" {
                policy.effective_ramen_failure = false;
                local.effective_ramen_failure = false
            } else if let Some(v) = token.strip_prefix("bridge") {
                local.safety_bridge_min_fail = v.parse()?
            } else if let Some(v) = token.strip_prefix("bgain") {
                local.safety_bridge_min_gain = v.parse()?
            } else if let Some(v) = token.strip_prefix("bcost") {
                local.safety_bridge_stock_cost = v.parse()?
            } else if token == "failmodel" {
                local.expected_fail = true
            } else if token == "vital" {
                local.dynamic_vital = true
            } else if token == "hintprob" {
                local.probabilistic_hint = true
            } else if token == "structall" {
                local.status_reserve_max = 40.;
                local.dynamic_vital = true;
                local.probabilistic_hint = true;
                local.expected_fail = true
            } else if token == "eager" {
                local.eager_eat = true
            } else if token == "plain" {
                local.early_bond_value = 0.;
                local.hint_bonus = 0.;
                local.first_friend_click_value = 0.;
                local.low_friend_bond_value = 0.;
                local.active_friend_value = 0.;
                local.overflow_value = 0.;
                m = true
            } else if token == "long" || token == "base" {
                m = true
            } else if let Some(v) = token.strip_prefix("pt") {
                policy.pt_rate = v.parse()?;
                p = true
            } else if let Some(v) = token.strip_prefix("sac") {
                local.max_base_score_sacrifice = v.parse()?;
                s = true
            } else if let Some(v) = token.strip_prefix("reserve") {
                local.status_reserve_max = v.parse()?
            } else if let Some(v) = token.strip_prefix("fail") {
                local.high_fail_penalty = v.parse()?;
                f = true
            } else if let Some(v) = token.strip_prefix("ck") {
                local.checkpoint_scale = v.parse::<f32>()? / 100.
            } else if let Some(v) = token.strip_prefix("rmj") {
                local.rmj_cross_bonus = v.parse()?
            } else if let Some(v) = token.strip_prefix("great") {
                local.great_cross_bonus = v.parse()?
            } else if let Some(v) = token.strip_prefix("rpt") {
                policy.ramen_pt_weight = v.parse::<f32>()? / 100.0
            } else if let Some(v) = token.strip_prefix("align") {
                local.ramen_lookahead_weight = v.parse::<f32>()? / 100.0
            } else if let Some(v) = token.strip_prefix("window") {
                local.ramen_window_weight = v.parse::<f32>()? / 100.0
            } else if let Some(v) = token.strip_prefix("look") {
                local.ramen_lookahead_weight = v.parse::<f32>()? / 100.0
            } else if let Some(v) = token.strip_prefix("samples") {
                local.ramen_lookahead_samples = v.parse()?
            } else {
                anyhow::bail!("未知矩阵变体字段: {token} ({name})")
            }
        }
        if !(p && s && m && f) {
            anyhow::bail!("矩阵变体字段不完整: {name}")
        }
        Ok(Self::with_configs(policy, local))
    }
    fn choose(o: &[RamenPolicyOutput]) -> usize {
        o.iter()
            .enumerate()
            .max_by(|(li, l), (ri, r)| l.score.total_cmp(&r.score).then_with(|| ri.cmp(li)))
            .map(|x| x.0)
            .unwrap_or(0)
    }
    fn stash(&self, o: &[RamenPolicyOutput]) {
        let t = o
            .iter()
            .enumerate()
            .map(|(i, x)| format!("#{i} {:.0}[{}]", x.score, x.reason))
            .collect::<Vec<_>>()
            .join(" | ");
        if let Ok(mut b) = self.last_breakdown.lock() {
            *b = Some(t)
        }
    }
    fn phase(turn: i32) -> f32 {
        if turn < 24 {
            1.
        } else if turn < 48 {
            0.55
        } else {
            0.15
        }
    }
    fn reserve_penalty(&self, g: &RamenGame, gain: &[i32; 6]) -> f32 {
        if self.config.status_reserve_max <= 0. {
            return 0.;
        }
        let rem = (76 - g.turn()).max(0) as f32;
        let r = self.config.status_reserve_max * rem / 76.;
        let mut p = 0.;
        for i in 0..5 {
            let h = (g.uma.five_status_limit[i] - g.uma.five_status[i]).max(0) as f32;
            let b = (r - h).max(0.);
            let a = (r - (h - gain[i] as f32)).max(0.);
            p += (a * a - b * b) / (2. * r.max(1.));
        }
        p * 6.
    }
    fn vital_factor(t: i32) -> f32 {
        if t >= 72 { 0.25 } else { 3.5 + (t as f32 / 72.) * 2. }
    }
    fn decide_train(&self, g: &RamenGame, a: &[RamenAction]) -> Result<(usize, Vec<RamenPolicyOutput>)> {
        let (guard, mut out) = self.policy.decide_train(g, a)?;
        if out.len() != a.len() {
            return Ok((guard, out));
        }
        let base = out.iter().map(|x| x.score).collect::<Vec<_>>();
        let bb = Self::choose(&out);
        let ph = Self::phase(g.turn());
        for (act, o) in a.iter().zip(out.iter_mut()) {
            let Operation::Train(tt) = act.operation else { continue };
            let tr = tt as usize;
            let buffs = g.calc_training_buff(tr)?;
            let val = g.calc_training_value(&buffs, tr)?;
            let people = g
                .distribution()
                .get(tr)
                .into_iter()
                .flatten()
                .copied()
                .filter(|&x| x >= 0 && (x as usize) < g.persons().len())
                .map(|x| x as usize)
                .collect::<Vec<_>>();
            let hn = people
                .iter()
                .filter(|&&i| g.persons()[i].hint() && matches!(g.persons()[i].person_type(), PersonType::Card))
                .count();
            let hp = if self.config.probabilistic_hint && hn > 0 {
                1. / hn as f32
            } else {
                1.
            };
            let mut lt = 0.;
            for i in people {
                let x = &g.persons()[i];
                match x.person_type() {
                    PersonType::ScenarioCard => {
                        lt += match g.friend.out_state {
                            FriendOutState::UnClicked => self.config.first_friend_click_value,
                            _ if x.friendship() < 60 => self.config.low_friend_bond_value * ph,
                            _ => self.config.active_friend_value,
                        }
                    }
                    PersonType::Card if x.friendship() < 80 => {
                        let mut b = if g.uma.flags.aijiao { 9. } else { 7. };
                        if x.hint() {
                            b += 5. * hp
                        }
                        b = b.min((80 - x.friendship()) as f32);
                        lt += b * self.config.early_bond_value * ph;
                        if x.hint() {
                            lt += self.config.hint_bonus * hp
                        }
                    }
                    PersonType::Card if x.hint() => lt += self.config.hint_bonus * hp,
                    _ => {}
                }
            }
            o.score += lt;
            o.add("local_long_term", lt);
            let rp = -self.reserve_penalty(g, &val.status_pt);
            o.score += rp;
            o.add("future_status_reserve", rp);
            if self.config.dynamic_vital {
                let c = (-val.vital).max(0) as f32;
                let z = -c * (Self::vital_factor(g.turn()) - self.policy.config.train_vital_value);
                o.score += z;
                o.add("dynamic_vital", z)
            }
            let base_fr = g.calc_training_failure_rate(&buffs, tr);
            let ramen_effect = calc_ramen_training_effect(g, tr, g.shining_count(tr) > 0);
            let fr = if self.config.effective_ramen_failure {
                (base_fr * (100.0 - ramen_effect.fail_rate_drop as f32) / 100.0).clamp(0.0, 100.0)
            } else {
                base_fr
            };
            if self.config.expected_fail && fr > 0. {
                let p = fr / 100.;
                let bp = if fr >= 20. { p } else { 0. };
                let z = -p * (150. + bp * 350. - self.policy.config.failure_penalty);
                o.score += z;
                o.add("expected_fail_layers", z)
            } else if fr > 15. && self.config.high_fail_penalty > 0. {
                let z = -((fr - 15.) / 85.).clamp(0., 1.) * self.config.high_fail_penalty;
                o.score += z;
                o.add("local_high_fail_tail", z)
            }
        }
        let lb = Self::choose(&out);
        let sacrifice = base[bb] - base[lb];
        let c = if sacrifice <= self.config.max_base_score_sacrifice {
            lb
        } else {
            bb
        };
        Ok((c, out))
    }
    fn pt_effect(pt: i32) -> Result<(i32, i32, i32)> {
        let d = RAMENDATA.get().ok_or_else(|| anyhow::anyhow!("RAMENDATA 未初始化"))?;
        let e = d
            .ramen_pt_effect
            .iter()
            .filter(|e| e.pt_min <= pt)
            .last()
            .or_else(|| d.ramen_pt_effect.first())
            .ok_or_else(|| anyhow::anyhow!("ramen_pt_effect 为空"))?;
        Ok((e.xunlian, e.deyilv, e.hint))
    }
    fn year_end(g: &RamenGame) -> i32 {
        if g.turn() < 24 {
            23
        } else if g.turn() < 48 {
            47
        } else {
            71
        }
    }
    fn scenario_threshold_value(&self, g: &RamenGame, post: i32) -> Result<(f32, f32, f32)> {
        let cur = g.ramen.scenario_pt;
        let rem = (Self::year_end(g) - g.turn()).max(0) as f32;
        let (a, b) = (Self::pt_effect(cur)?, Self::pt_effect(post)?);
        // 训练加成最直接，得意率与 Hint 使用较低近似权重；乘年度剩余回合表达提前跨档的持续价值。
        let delta = ((b.0 - a.0) as f32 * 4. + (b.1 - a.1) as f32 * 0.8 + (b.2 - a.2) as f32 * 0.4).max(0.);
        let region_delta = (calc_region_bonus(post) - calc_region_bonus(cur)).max(0) as f32 * 8.;
        let checkpoint = (delta + region_delta) * rem * self.config.checkpoint_scale;
        let year = (g.current_year() - 1) as usize;
        let d = RAMENDATA.get().unwrap();
        let threshold = *d.ramen_success_pt.get(year).unwrap_or(&i32::MAX);
        let rmj = if cur < threshold && post >= threshold {
            self.config.rmj_cross_bonus
        } else {
            0.
        };
        let great = if year == 2 && cur < 5000 && post >= 5000 {
            self.config.great_cross_bonus
        } else {
            0.
        };
        Ok((checkpoint, rmj, great))
    }
    fn best_action_score(&self, g: &RamenGame) -> Result<f32> {
        let actions = g.list_actions()?;
        let (idx, out) = self.decide_train(g, &actions)?;
        // 守门返回单项 MAX；吃面通常不改变治病/休息等守门结论，因此不把 MAX 计入前向增量。
        if out.len() != actions.len() {
            return Ok(0.0);
        }
        Ok(out.get(idx).map(|x| x.score).unwrap_or(0.0))
    }
    /// 精确复原 v8 的吃面前窗口信号，用于解释其收益来源。
    /// 它只查看候选地区 at_trains 当前已有的真实训练窗口，不预测分身。
    fn ramen_window_alignment(&self, g: &RamenGame, region_id: usize) -> Result<f32> {
        if self.config.ramen_window_weight <= 0.0 {
            return Ok(0.0);
        }
        let d = RAMENDATA.get().ok_or_else(|| anyhow::anyhow!("RAMENDATA 未初始化"))?;
        let region = d
            .ramen_region_effect
            .get(region_id)
            .ok_or_else(|| anyhow::anyhow!("地区效果缺失: {region_id}"))?;
        let mut best = 0.0f32;
        for &t in &region.at_trains {
            if !(0..5).contains(&t) {
                continue;
            }
            let tr = t as usize;
            let buffs = g.calc_training_buff(tr)?;
            let v = g.calc_training_value(&buffs, tr)?;
            let raw = v.status_pt[..5].iter().sum::<i32>() as f32 + v.status_pt[5] as f32 * 2.0;
            let people = g.distribution().get(tr).map(|x| x.len()).unwrap_or(0) as f32;
            let shining = g.shining_count(tr) as f32;
            best = best.max(raw + people * 8.0 + shining * 35.0);
        }
        let effect = (region.xunlian + region.youqing + region.pt_bonus) as f32 + region.hint_count as f32 * 10.0;
        Ok(best * effect * self.config.ramen_window_weight / 100.0)
    }
    /// 在真正吃面前，用状态副本执行候选面并评估其事后最佳动作。
    /// 所有 region_id 走同一逻辑；不按人数、彩圈或拉面名称硬编码排序。
    fn ramen_lookahead(&self, g: &RamenGame, region_id: usize) -> Result<f32> {
        if self.config.ramen_lookahead_weight <= 0.0 {
            return Ok(0.0);
        }
        let mut no_eat = g.clone();
        no_eat.stage = RamenStage::Train;
        no_eat.ramen.current_ramen = None;
        no_eat.ramen.clear_pending();
        let baseline = self.best_action_score(&no_eat)?;
        let targets = list_special_targets_for(&g.ramen, region_id)?
            .into_iter()
            .min_by_key(|t| t.iter().sum::<i32>())
            .ok_or_else(|| anyhow::anyhow!("拉面 {region_id} 没有合法诀窍方案"))?;
        let n = self.config.ramen_lookahead_samples.max(1);
        let mut total = 0.0;
        for sample in 0..n {
            let mut preview = g.clone();
            preview.ramen.current_ramen = None;
            preview.ramen.pending_ramen = Some(region_id);
            preview.ramen.pending_special_targets = targets;
            // 种子只由吃面前已知状态、候选和样本编号构成；不会读取真实策略流的落点。
            let seed = (g.turn() as u64).wrapping_mul(0x9E3779B97F4A7C15)
                ^ (g.ramen.scenario_pt as u64).rotate_left(17)
                ^ ((region_id as u64) << 32)
                ^ sample as u64;
            let mut rng = StdRng::seed_from_u64(seed);
            preview.ground_ramen_effects(&mut rng)?;
            preview.stage = RamenStage::Train;
            // decide_train 会用 calc_training_buff/value/failure 对全部五个训练和其他合法动作统一评分。
            total += self.best_action_score(&preview)?;
        }
        Ok((total / n as f32 - baseline) * self.config.ramen_lookahead_weight)
    }
    /// Detect a narrow Y1 safety transition. The normal train policy stays conservative
    /// (raw failure); this only asks whether the shared 30% reduction would make a risky
    /// training overtake the current best action. If any craftable ramen already covers that
    /// training, normal window alignment owns the decision and this bridge stays off.
    fn safety_bridge(&self, g: &RamenGame, ramen_actions: &[RamenAction]) -> Result<Option<(usize, f32)>> {
        if g.current_year() != 1 || self.config.safety_bridge_min_fail > 100.0 {
            return Ok(None);
        }
        let mut preview = g.clone();
        preview.stage = RamenStage::Train;
        let actions = preview.list_actions()?;
        let (_, outs) = self.policy.decide_train(&preview, &actions)?;
        if outs.len() != actions.len() {
            return Ok(None);
        }
        let raw_best = outs.iter().map(|x| x.score).fold(f32::NEG_INFINITY, f32::max);
        let mut rescued: Option<(usize, f32)> = None;
        for (act, out) in actions.iter().zip(outs.iter()) {
            let Operation::Train(tt) = act.operation else { continue };
            let tr = tt as usize;
            let buffs = preview.calc_training_buff(tr)?;
            let fr = preview.calc_training_failure_rate(&buffs, tr);
            if fr < self.config.safety_bridge_min_fail {
                continue;
            }
            let fail_adj = out
                .breakdown
                .iter()
                .find(|(k, _)| k == "fail_adj")
                .map(|(_, v)| *v)
                .unwrap_or(0.0);
            let gross = out.score - fail_adj;
            let effective_fr = fr * 0.70;
            let effective_adj =
                -(gross * effective_fr / 100.0 + self.policy.config.failure_penalty * effective_fr / 100.0);
            let effective_score = gross + effective_adj;
            let gain = effective_score - raw_best;
            if gain >= self.config.safety_bridge_min_gain && rescued.map(|(_, old)| gain > old).unwrap_or(true) {
                rescued = Some((tr, gain));
            }
        }
        let Some((tr, gain)) = rescued else {
            return Ok(None);
        };
        let d = RAMENDATA.get().ok_or_else(|| anyhow::anyhow!("RAMENDATA 未初始化"))?;
        let covered = ramen_actions.iter().filter_map(|x| x.ramen).any(|rid| {
            d.ramen_region_effect
                .get(rid)
                .map(|r| r.at_trains.contains(&(tr as i32)))
                .unwrap_or(false)
        });
        Ok(if covered { None } else { Some((tr, gain)) })
    }

    fn safety_bridge_candidate(&self, g: &RamenGame, region_id: usize, gain: f32) -> Result<f32> {
        let targets = list_special_targets_for(&g.ramen, region_id)?
            .into_iter()
            .min_by_key(|t| t.iter().sum::<i32>())
            .ok_or_else(|| anyhow::anyhow!("拉面 {region_id} 无合法 targets"))?;
        let used = targets.iter().sum::<i32>() as f32;
        let before = g
            .ramen
            .selected_regions
            .iter()
            .filter(|&&rid| {
                list_special_targets_for(&g.ramen, rid)
                    .map(|x| !x.is_empty())
                    .unwrap_or(false)
            })
            .count();
        let mut post = g.ramen.clone();
        consume_for_ramen(&mut post, region_id, &targets)?;
        let after = g
            .ramen
            .selected_regions
            .iter()
            .filter(|&&rid| {
                list_special_targets_for(&post, rid)
                    .map(|x| !x.is_empty())
                    .unwrap_or(false)
            })
            .count();
        let lost = before.saturating_sub(after) as f32;
        Ok(gain - (lost + used) * self.config.safety_bridge_stock_cost)
    }

    fn decide_ramen(&self, g: &RamenGame, a: &[RamenAction]) -> Result<(usize, Vec<RamenPolicyOutput>)> {
        let (_, mut out) = self.policy.decide_ramen(g, a)?;
        let risk = (g.ramen.feeling_stock.iter().sum::<i32>() - self.config.feeling_overflow_threshold).max(0) as f32;
        let bridge = self.safety_bridge(g, a)?;
        for (act, o) in a.iter().zip(out.iter_mut()) {
            if let Some(region_id) = act.ramen {
                let pressure = risk * self.config.overflow_value;
                o.score += pressure;
                o.add("local_stock_pressure", pressure);
                let y = (g.current_year() - 1) as usize;
                let post = g.ramen.scenario_pt + calc_ramen_pt_gain(y, g.ramen.eat_count)?;
                let (ck, rmj, great) = self.scenario_threshold_value(g, post)?;
                let window = self.ramen_window_alignment(g, region_id)?;
                let safety = if let Some((_, gain)) = bridge {
                    self.safety_bridge_candidate(g, region_id, gain)?
                } else {
                    0.0
                };
                let look = self.ramen_lookahead(g, region_id)?;
                o.score += ck + rmj + great + window + safety + look;
                o.add("scenario_checkpoint", ck);
                o.add("rmj_cross", rmj);
                o.add("great_cross", great);
                o.add("ramen_window", window);
                o.add("safety_bridge", safety);
                o.add("ramen_lookahead", look)
            }
        }
        // 吃不吃与吃哪碗分层：eager 模式下，只要 RamenSelect 已列出可制作面，
        // 就在这些面之间 argmax；不扩展 selected_regions，也不枚举年度其他地区。
        // 吃完后的 Train 阶段仍根据真实落地状态重新比较全部合法动作。
        let chosen = if self.config.eager_eat {
            a.iter()
                .zip(out.iter())
                .enumerate()
                .filter(|(_, (act, _))| act.ramen.is_some())
                .max_by(|(li, (_, l)), (ri, (_, r))| l.score.total_cmp(&r.score).then_with(|| ri.cmp(li)))
                .map(|(i, _)| i)
                .unwrap_or_else(|| Self::choose(&out))
        } else {
            Self::choose(&out)
        };
        Ok((chosen, out))
    }
}
impl Trainer<RamenGame> for LocalRamenTrainer {
    fn select_action(&self, g: &RamenGame, a: &[RamenAction], _r: &mut StdRng) -> Result<usize> {
        if a.len() <= 1 {
            return Ok(0);
        }
        let (c, o) = match g.stage {
            RamenStage::Train => self.decide_train(g, a)?,
            RamenStage::RamenSelect => self.decide_ramen(g, a)?,
            RamenStage::SpecialSelect => self.policy.decide_special(g, a)?,
            RamenStage::RegionSelect => {
                let y = match g.turn() {
                    2 => 0,
                    23 => 1,
                    47 => 2,
                    _ => 0,
                };
                self.policy.decide_region(g, y, a)?
            }
            _ => (0, Vec::new()),
        };
        self.stash(&o);
        Ok(c)
    }
    fn select_choice(&self, g: &RamenGame, c: &[Vec<EventChoice>], _r: &mut StdRng) -> Result<usize> {
        let (i, o) = self.policy.decide_event(g, c)?;
        self.stash(&o);
        Ok(i)
    }
    fn select_event_choice(
        &self, g: &RamenGame, _e: &EventData, c: &[Vec<EventChoice>], r: &mut StdRng,
    ) -> Result<usize> {
        self.select_choice(g, c, r)
    }
    fn last_breakdown(&self) -> Option<String> {
        self.last_breakdown.lock().ok().and_then(|b| b.clone())
    }
}
