//! 拉面杯实验策略：在现有即时评分上增加长期训练结构与剧本 PT 阈值价值。
use crate::{
    game::{
        FriendOutState, Game, Person, PersonType, Trainer,
        ramen::{
            Operation, RamenAction, RamenGame, RamenStage,
            effects::calc_ramen_training_effect,
            policy::{RamenPolicy, RamenPolicyConfig, RamenPolicyOutput},
            rules::{calc_ramen_pt_gain, calc_region_bonus, consume_for_ramen, get_recipe, list_special_targets_for},
        },
    },
    gamedata::{EventChoice, EventData, ramen::RAMENDATA},
};
use anyhow::Result;
use rand::{SeedableRng, prelude::StdRng};
use std::sync::Mutex;

#[derive(Debug, Clone)]
pub struct LocalRamenConfig {
    /// 低于 80 羁绊的普通支援卡每获得 1 点羁绊所折算的长期评分。
    ///
    /// 实际加分还会乘年度衰减系数，并受距离 80 羁绊的剩余空间限制；单位为策略评分/羁绊点。
    /// 设为 `0.0` 可关闭普通支援卡的早期羁绊估值。
    pub early_bond_value: f32,

    /// 点击带 Hint 支援卡时附加的即时 Hint 价值，单位为策略评分。
    ///
    /// 启用 [`Self::probabilistic_hint`] 时，会除以当前训练中带 Hint 的卡数，表达随机命中概率。
    pub hint_bonus: f32,

    /// 首次点击剧本友人卡、使其从未点击状态向外出解锁推进的长期价值，单位为策略评分。
    pub first_friend_click_value: f32,

    /// 剧本友人卡已点击但羁绊低于 60 时，每次点击的长期价值，单位为策略评分。
    ///
    /// 该值会乘年度衰减系数，避免后期继续高估尚未解锁完成的友人链。
    pub low_friend_bond_value: f32,

    /// 剧本友人卡进入活跃阶段后的每次点击价值，单位为策略评分。
    pub active_friend_value: f32,

    /// 旧版高失败率尾部惩罚的最大值，单位为策略评分。
    ///
    /// 仅当 [`Self::expected_fail`] 为 `false` 且基础失败率高于 15% 时使用；
    /// `0.0` 表示关闭该旧模型。当前推荐配置使用期望失败模型，保留此字段仅供消融实验。
    pub high_fail_penalty: f32,

    /// 诀窍总库存超过该数量后，开始给“立即吃面”增加溢出压力。
    ///
    /// 单位为诀窍个数；例如默认值 `8` 表示库存总数从第 9 个开始产生压力。
    pub feeling_overflow_threshold: i32,

    /// 每个超过 [`Self::feeling_overflow_threshold`] 的诀窍所产生的吃面奖励，单位为策略评分/诀窍。
    ///
    /// 用于避免库存接近上限时继续等待而丢弃最早获得的诀窍；`0.0` 表示关闭。
    pub overflow_value: f32,

    /// 长期结构评分相对基础即时评分最多允许牺牲的分数，单位为策略评分。
    ///
    /// 如果长期结构选出的动作比基础策略最佳动作低超过该值，则回退到基础策略动作，
    /// 防止羁绊、Hint 等启发式为了长期收益牺牲过多本回合收益。
    pub max_base_score_sacrifice: f32,

    /// 为未来固定事件和终盘奖励预留的最大属性空间，单位为原始属性点。
    ///
    /// 预留量会随剩余回合线性缩小；训练把属性推近上限时会产生软惩罚。
    /// `0.0` 表示关闭属性溢出预留模型。
    pub status_reserve_max: f32,

    /// 是否使用随回合变化的体力成本模型。
    ///
    /// `true` 时前期体力消耗更贵，终盘体力价值逐渐降低，并只补上相对基础策略
    /// `train_vital_value` 尚未计入的差额。
    pub dynamic_vital: bool,

    /// 是否把多个同时亮起的 Hint 视为随机命中，而不是每个 Hint 都按全额价值计算。
    pub probabilistic_hint: bool,

    /// 是否使用连续的失败期望损失模型。
    ///
    /// `true` 时按失败概率扣除小失败损失，并在失败率达到 20% 后加入大失败尾部风险；
    /// `false` 时可选择使用 [`Self::high_fail_penalty`] 的旧阈值模型。
    pub expected_fail: bool,

    /// 吃面跨越 `scenario_pt` 常驻效果档位后的持续价值倍率。
    ///
    /// 先计算训练加成、得意率、Hint 与地区词条的档位差，再乘当年剩余回合和该倍率。
    /// 这是无量纲缩放系数；`0.0` 表示关闭档位前瞻价值。
    pub checkpoint_scale: f32,

    /// 本次吃面首次跨过当年 RMJ 成功线时的一次性奖励，单位为策略评分。
    ///
    /// 只在吃面前低于成功线且吃面后达到或超过成功线时计入一次。
    pub rmj_cross_bonus: f32,

    /// 第三年本次吃面首次跨过 5000 剧本 PT 大成功线时的一次性奖励，单位为策略评分。
    pub great_cross_bonus: f32,

    /// 随机事后前向值的权重，无量纲。
    ///
    /// 前向会在状态副本中执行候选拉面、随机落地分身并比较吃面前后的最佳动作。
    /// 实验表明它会干扰当前真实训练窗口，当前推荐值为 `0.0`；保留字段用于回归消融。
    pub ramen_lookahead_weight: f32,

    /// 每个候选拉面执行随机事后前向时使用的独立样本数。
    ///
    /// 仅当 [`Self::ramen_lookahead_weight`] 大于 `0.0` 时生效；最小按 1 个样本处理。
    pub ramen_lookahead_samples: usize,

    /// 是否强制在存在可制作拉面时从拉面候选中选择，而不允许“不吃面”参与竞争。
    ///
    /// 该模式只用于实验；当前推荐策略为 `false`，由窗口价值正常决定吃面时机。
    pub eager_eat: bool,

    /// 当前真实训练窗口与候选拉面覆盖训练的耦合权重，无量纲。
    ///
    /// 窗口由当前训练原始收益、人数和彩圈数构成，再乘地区效果强度。
    /// 这是 v8 高收益的主要来源；配置 token `window10` 对应 `0.10`。
    pub ramen_window_weight: f32,

    /// 策略评分是否采用吃面后的实际失败率下降。
    ///
    /// `true` 按当年拉面效果降低失败率；`false` 使用吃面前基础失败率作为保守风险预算。
    /// 游戏规则执行始终使用真实失败率，本开关只影响动作评分。
    pub effective_ramen_failure: bool,

    /// 第一年安全过渡门控允许救援的训练最低基础失败率，单位为百分比。
    ///
    /// 大于 `100.0` 表示完全关闭该实验功能；当前默认 `101.0` 即关闭。
    pub safety_bridge_min_fail: f32,

    /// 应用第一年 30% 相对失败率下降后，风险训练超过当前最佳动作所需的最低增益。
    ///
    /// 单位为策略评分，仅在安全过渡门控启用时生效。
    pub safety_bridge_min_gain: f32,

    /// 安全过渡选择拉面时，每损失一个事后可制作选项或消耗一个隐藏风味的成本。
    ///
    /// 单位为策略评分/资源单位，仅在安全过渡门控启用时生效。
    pub safety_bridge_stock_cost: f32,

    /// 田园杯 Cook2 凹函数材料估值适配到拉面诀窍库存后的总权重。
    ///
    /// 对 A/B/C 分别计算 `sqrt(吃前库存+2)-sqrt(吃后库存+2)`，隐藏风味另计灵活性成本，
    /// 再乘年度剩余比例与 RMJ 进度折扣。单位为策略评分缩放；当前最佳 `cook2-40` 为 `40.0`。
    pub cook2_stock_weight: f32,
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
            cook2_stock_weight: 0.0,
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
            } else if let Some(v) = token.strip_prefix("cook2") {
                local.cook2_stock_weight = v.parse()?
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

    /// Adaptation of Cook2::materialEvaluation. A unit from a scarce stock is worth more
    /// than one from a rich stock (concave sqrt utility). Unlike the farm scenario, ramen stock
    /// resets yearly, so its shadow price decays toward the RMJ boundary. Before reaching the
    /// annual success target we discount the price: spending to secure scenario progression is
    /// deliberately preferred, matching Cook2 Y1's aggressive cooking-until-target rule.
    fn cook2_ramen_stock_cost(&self, g: &RamenGame, region_id: usize) -> Result<f32> {
        if self.config.cook2_stock_weight <= 0.0 {
            return Ok(0.0);
        }
        let targets = list_special_targets_for(&g.ramen, region_id)?
            .into_iter()
            .min_by_key(|t| t.iter().sum::<i32>())
            .ok_or_else(|| anyhow::anyhow!("拉面 {region_id} 无合法 targets"))?;
        let recipe = get_recipe(region_id)?;
        let net = [recipe[0] - targets[0], recipe[1] - targets[1], recipe[2] - targets[2]];
        let year_end = Self::year_end(g);
        let remaining_fraction = ((year_end - g.turn()).max(0) as f32 / 21.0).clamp(0.0, 1.0);
        let year = (g.current_year() - 1) as usize;
        let d = RAMENDATA.get().ok_or_else(|| anyhow::anyhow!("RAMENDATA 未初始化"))?;
        let target = *d.ramen_success_pt.get(year).unwrap_or(&i32::MAX);
        let progression_discount = if g.ramen.scenario_pt < target { 0.35 } else { 1.0 };
        let mut marginal = 0.0;
        for i in 0..3 {
            let before = g.ramen.feeling_stock[i] as f32;
            let after = (g.ramen.feeling_stock[i] - net[i]).max(0) as f32;
            // Bias keeps the derivative finite, as in Cook2's sqrt(count + bias).
            marginal += (before + 2.0).sqrt() - (after + 2.0).sqrt();
        }
        // Hidden flavor is globally flexible, so charge it as two ordinary marginal units.
        let hidden = targets.iter().sum::<i32>() as f32;
        marginal += hidden * 0.50;
        Ok(marginal * self.config.cook2_stock_weight * remaining_fraction * progression_discount)
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
                let cook2_cost = self.cook2_ramen_stock_cost(g, region_id)?;
                let safety = if let Some((_, gain)) = bridge {
                    self.safety_bridge_candidate(g, region_id, gain)?
                } else {
                    0.0
                };
                let look = self.ramen_lookahead(g, region_id)?;
                o.score += ck + rmj + great + window + safety + look - cook2_cost;
                o.add("scenario_checkpoint", ck);
                o.add("rmj_cross", rmj);
                o.add("great_cross", great);
                o.add("ramen_window", window);
                o.add("cook2_stock_cost", -cook2_cost);
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
