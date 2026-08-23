//! 拉面杯实验策略：在现有即时评分上增加长期训练结构与剧本 PT 阈值价值。
use std::sync::Mutex;

use anyhow::Result;
use rand::{SeedableRng, prelude::StdRng};

use crate::{
    game::{
        FriendOutState,
        Game,
        Person,
        PersonType,
        Trainer,
        ramen::{
            Operation,
            RamenAction,
            RamenGame,
            RamenStage,
            effects::calc_ramen_training_effect,
            policy::{RamenPolicy, RamenPolicyConfig, RamenPolicyOutput},
            rules::{calc_ramen_pt_gain, calc_region_bonus, consume_for_ramen, get_recipe, list_special_targets_for}
        }
    },
    gamedata::{EventChoice, EventData, ramen::RAMENDATA}
};

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

    /// 是否把“吃面”和“本回合训练”视为不可拆分的事务。
    ///
    /// `true` 时先在不吃面的当前局面决定基础动作：若应休息、外出、治病或比赛，
    /// RamenSelect 直接选择不吃；一旦已经吃面，Train 阶段只在五种训练中比较，
    /// 不允许随后休息而浪费仅本回合生效的拉面加成。
    pub eat_requires_training: bool,

    /// 第三年吃面前希望具备的训练前体力，单位为体力点。
    ///
    /// 它回答“现在是否应该先恢复”。低于目标不会直接禁止吃面，而会按短缺量收费，
    /// 使极强窗口仍可突破保守线。`0` 表示关闭训练前体力预算。
    pub y3_pre_train_vital_target: i32,

    /// 第三年吃面并完成计划训练后希望保留的体力，单位为体力点。
    ///
    /// 它回答“本次训练会不会使下一回合崩盘”。智力训练同样参与计算，但因其体力变化
    /// 通常为正，训练后短缺自然较小；不再给予无条件豁免。`0` 表示关闭训练后预算。
    pub y3_post_train_vital_target: i32,

    /// 第三年训练前/后体力每短缺 1 点对候选面的软惩罚，单位为策略评分/体力点。
    ///
    /// 总成本为 `max(pre_target-V0,0) + max(post_target-V1,0)` 再乘此权重。
    /// `0.0` 表示关闭联合体力预算。
    pub y3_vital_shortfall_weight: f32,

    /// 第三年非智力训练后的极端安全底线，低于该值才硬禁止吃面。
    ///
    /// 与软目标分离：正常体力不足只扣分，只有接近打空时才保下限。智力训练也必须满足
    /// `V1 >= 0`，但不受此非智力硬底线。`0` 表示不额外硬拦。
    pub y3_post_train_hard_floor: i32,

    /// 是否按“距离下一次确定恢复前还有几个可训练回合”判断第三年体力崩盘。
    ///
    /// 当前规则中 turn=70 训练后，turn=71 为有马纪念，赛后固定恢复 40；随后
    /// turn=72 起超级拉面每回合开始恢复 20。因此 turn=70 可以把体力控到 0，
    /// 不应再为训练后低体力付费。更早回合若低体力会影响至少一个普通训练回合，
    /// 才计入崩盘成本。
    pub y3_recovery_horizon: bool,

    /// 当体力守门或正常打分原本选择休息时，是否优先用尚未完成的友人外出替代。
    ///
    /// 友人外出同样恢复体力，同时提供属性、干劲、Hint、隐藏风味和事件链进度；
    /// 仅替换本来就会消耗的休息回合，不为了赶链强行覆盖高价值训练。
    pub friend_outing_replaces_rest: bool,

    /// 友人第三次外出时，当前体力低于该值就选择恢复 50 体力的选项。
    ///
    /// 否则保留事件通用评分，可选无回复的属性/PT选项。`0` 表示关闭该低体力保护。
    pub friend_outing3_recovery_vital: i32,

    /// 各年结束前允许累计使用的友人外出次数上限。
    ///
    /// 五次外出是整局有限资源，每次还产生 2 个万能材料；不能因为第一年休息较多就一次用完。
    /// 例如 `[1, 3, 5]` 表示第一年最多用 1 次、第二年结束前最多累计 3 次、第三年可用完。
    /// `[5, 5, 5]` 等价于不做跨年配额；仅在 `friend_outing_replaces_rest=true` 时生效。
    pub friend_outing_cumulative_caps: [usize; 3],

    /// “休息→友人外出”替代时允许的最高当前万能材料数量。
    ///
    /// 外出固定获得 2 个万能材料且上限为 4；设为 2 可避免替代路径产生材料溢出。
    /// 原策略主动选择友人外出不受此门控，只受总次数配额约束。`4` 表示关闭。
    pub friend_rest_max_special: i32,

    /// RMJ/第三年5000目标在截止前的可达性紧迫度。
    pub deadline_urgency_scale: f32,

    /// SpecialSelect 是否按吃后库存、后续可制作集合和年末剩余价值动态选择。
    pub dynamic_special_targets: bool
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
            eat_requires_training: false,
            y3_pre_train_vital_target: 0,
            y3_post_train_vital_target: 0,
            y3_vital_shortfall_weight: 0.0,
            y3_post_train_hard_floor: 0,
            y3_recovery_horizon: false,
            friend_outing_replaces_rest: false,
            friend_outing3_recovery_vital: 0,
            friend_outing_cumulative_caps: [5, 5, 5],
            friend_rest_max_special: 4,
            deadline_urgency_scale: 0.0,
            dynamic_special_targets: false
        }
    }
}
pub struct LocalRamenTrainer {
    policy: RamenPolicy,
    config: LocalRamenConfig,
    last_breakdown: Mutex<Option<String>>
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
            last_breakdown: Mutex::new(None)
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
            } else if let Some(v) = token.strip_prefix("vrest") {
                policy.vital_rest = v.parse()?
            } else if token == "eatguard" {
                local.eat_requires_training = true
            } else if let Some(v) = token.strip_prefix("y3pre") {
                local.y3_pre_train_vital_target = v.parse()?
            } else if let Some(v) = token.strip_prefix("y3post") {
                local.y3_post_train_vital_target = v.parse()?
            } else if let Some(v) = token.strip_prefix("y3vw") {
                local.y3_vital_shortfall_weight = v.parse()?
            } else if let Some(v) = token.strip_prefix("y3hard") {
                local.y3_post_train_hard_floor = v.parse()?
            } else if token == "y3horizon" {
                local.y3_recovery_horizon = true
            } else if token == "friendrest" {
                local.friend_outing_replaces_rest = true
            } else if let Some(v) = token.strip_prefix("friend3v") {
                local.friend_outing3_recovery_vital = v.parse()?
            } else if let Some(v) = token.strip_prefix("friendcap") {
                let digits = v.as_bytes();
                if digits.len() != 3 || !digits.iter().all(u8::is_ascii_digit) {
                    anyhow::bail!("friendcap 必须是三个数字，如 135: {v}");
                }
                local.friend_outing_cumulative_caps = [
                    (digits[0] - b'0') as usize,
                    (digits[1] - b'0') as usize,
                    (digits[2] - b'0') as usize
                ];
                let c = local.friend_outing_cumulative_caps;
                if c[0] > c[1] || c[1] > c[2] || c[2] > 5 {
                    anyhow::bail!("friendcap 必须单调且不超过5: {v}");
                }
            } else if let Some(v) = token.strip_prefix("friendspecial") {
                local.friend_rest_max_special = v.parse()?
            } else if let Some(v) = token.strip_prefix("deadline") {
                local.deadline_urgency_scale = v.parse::<f32>()? / 100.0
            } else if token == "specialdynamic" {
                local.dynamic_special_targets = true
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
    /// 本年是否仍有友人外出配额。配额按整局累计次数控制，而不是每年重置。
    fn friend_outing_within_pacing(&self, g: &RamenGame) -> bool {
        let year = (g.current_year() - 1).clamp(0, 2) as usize;
        let used = g.friend.out_used.iter().filter(|&&x| x).count();
        used < self.config.friend_outing_cumulative_caps[year]
    }

    /// 下一段友人外出的动态价值。
    ///
    /// 事件本体按当前体力/干劲裁掉溢出，第三段两个选项也在这里实时比较；万能材料固定按
    /// 2 个来源计价，即使当前计数已满也不把外出禁掉。跨年稀缺性只由累计配额控制。
    fn dynamic_friend_outing_value(&self, g: &RamenGame) -> Result<(f32, Vec<(String, f32)>, String)> {
        let used = g.friend.out_used.iter().filter(|&&x| x).count();
        if used >= 5 {
            return Ok((f32::NEG_INFINITY, vec![], "友人外出已完成".to_string()));
        }
        let data = RAMENDATA.get().ok_or_else(|| anyhow::anyhow!("RAMENDATA 未初始化"))?;
        let event = data
            .friend_events
            .get(&format!("outing{}", used + 1))
            .ok_or_else(|| anyhow::anyhow!("缺少友人外出事件 {}", used + 1))?;
        let (choice, event_value) = self.dynamic_friend_event_choice(g, &event.choices)?;

        // friend_outing_bonus 原本把“2万能材料+事件链”压成一个固定值。这里保留总尺度，
        // 但拆为固定材料来源价值和随段数/年份上升的完链价值。
        let material = self.policy.config.friend_outing_bonus * (2.0 / 3.0);
        let chain_urgency = 0.70 + used as f32 * 0.12 + (g.current_year() - 1) as f32 * 0.18;
        let chain = self.policy.config.friend_outing_bonus * (1.0 / 3.0) * chain_urgency;
        let base = self.policy.config.outing_base;
        let total = base + event_value + material + chain;
        Ok((
            total,
            vec![
                ("outing_base".to_string(), base),
                ("friend_event_dynamic".to_string(), event_value),
                ("friend_material_required".to_string(), material),
                ("friend_chain_dynamic".to_string(), chain),
            ],
            format!(
                "友人外出#{} 选项{} 动态事件{:.0} 材料+2(库存{}也不禁用)",
                used + 1,
                choice + 1,
                event_value,
                g.ramen.special_feeling
            )
        ))
    }

    /// 按当前状态给友人事件选项评分。先复用通用事件评分，再扣除体力/干劲实际无法获得的
    /// 溢出；最大体力是永久收益，补回通用事件评分尚未覆盖的价值。
    fn dynamic_friend_event_choice(&self, g: &RamenGame, choices: &[Vec<EventChoice>]) -> Result<(usize, f32)> {
        let (_, base) = self.policy.decide_event(g, choices)?;
        let mut values = Vec::with_capacity(choices.len());
        for (group, out) in choices.iter().zip(base.iter()) {
            let mut adjust = 0.0;
            for c in group {
                let prob = if c.prob == 0 { 1.0 } else { c.prob as f32 / 100.0 };
                let max_after = g.uma.max_vital + c.value.max_vital;
                let requested_vital = c.value.vital.max(0);
                let realized_vital = requested_vital.min((max_after - g.uma.vital).max(0));
                adjust -= (requested_vital - realized_vital) as f32 * self.policy.config.event_vital_weight * prob;
                let requested_motivation = c.value.motivation.max(0);
                let realized_motivation = requested_motivation.min((5 - g.uma.motivation).max(0));
                adjust -= (requested_motivation - realized_motivation) as f32
                    * self.policy.config.event_motivation_weight
                    * prob;
            }
            values.push(out.score + adjust);
        }
        let choice = values
            .iter()
            .enumerate()
            .max_by(|(li, l), (ri, r)| l.total_cmp(r).then_with(|| ri.cmp(li)))
            .map(|(i, _)| i)
            .unwrap_or(0);
        Ok((choice, values.get(choice).copied().unwrap_or(0.0)))
    }

    fn decide_train(&self, g: &RamenGame, a: &[RamenAction]) -> Result<(usize, Vec<RamenPolicyOutput>)> {
        let (mut guard, mut out) = self.policy.decide_train(g, a)?;
        let recovery_guard = self.config.friend_outing_replaces_rest
            && a.get(guard).is_some_and(|x| x.operation == Operation::Rest)
            && out.len() != a.len();
        if recovery_guard && a.iter().any(|x| x.operation == Operation::FriendOuting) {
            // 展开完整候选以便真正执行五段动态估值；最终仍只允许休息/友人恢复动作获胜。
            out = self.policy.score_train_actions(g, a)?;
            guard = a.iter().position(|x| x.operation == Operation::Rest).unwrap_or(guard);
        }
        if out.len() != a.len() {
            let ate_this_turn = self.config.eat_requires_training && g.ramen.current_ramen.is_some();
            let selected_is_train = a
                .get(guard)
                .is_some_and(|action| matches!(action.operation, Operation::Train(_)));
            if !ate_this_turn || selected_is_train {
                return Ok((guard, out));
            }
            // 已吃面但旧硬守门想休息/外出：重新计算全部候选，并只允许五种训练。
            // 生病/自选比赛通常不会经过吃面前门控；这里仍以“拉面只为训练使用”为最终不变量。
            out = self.policy.score_train_actions(g, a)?;
            guard = out
                .iter()
                .enumerate()
                .filter(|(i, _)| a.get(*i).is_some_and(|x| matches!(x.operation, Operation::Train(_))))
                .max_by(|(li, l), (ri, r)| l.score.total_cmp(&r.score).then_with(|| ri.cmp(li)))
                .map(|(i, _)| i)
                .ok_or_else(|| anyhow::anyhow!("已吃面但 Train 阶段没有训练候选"))?;
        }
        if let Some(friend_idx) = a.iter().position(|x| x.operation == Operation::FriendOuting) {
            let (score, breakdown, reason) = self.dynamic_friend_outing_value(g)?;
            if let Some(friend) = out.get_mut(friend_idx) {
                friend.score = score;
                friend.breakdown = breakdown;
                friend.reason = reason;
            }
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
            let all_hint = g.is_hint_special_active_for_train(tr);
            let hp = if self.config.probabilistic_hint && hn > 0 && !all_hint {
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
                            _ => self.config.active_friend_value
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
                            let repeats = if all_hint && i < g.deck().len() {
                                1 + g.deck()[i].effect.hint_count_bonus
                            } else {
                                1
                            };
                            lt += self.config.hint_bonus * hp * repeats as f32
                        }
                    }
                    PersonType::Card if x.hint() => {
                        let repeats = if all_hint && i < g.deck().len() {
                            1 + g.deck()[i].effect.hint_count_bonus
                        } else {
                            1
                        };
                        lt += self.config.hint_bonus * hp * repeats as f32
                    }
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
        let mut c = if sacrifice <= self.config.max_base_score_sacrifice {
            lb
        } else {
            bb
        };
        if recovery_guard {
            c = out
                .iter()
                .enumerate()
                .filter(|(i, _)| {
                    a.get(*i).is_some_and(|x| {
                        x.operation == Operation::Rest
                            || (x.operation == Operation::FriendOuting && self.friend_outing_within_pacing(g))
                    })
                })
                .max_by(|(li, l), (ri, r)| l.score.total_cmp(&r.score).then_with(|| ri.cmp(li)))
                .map(|(i, _)| i)
                .ok_or_else(|| anyhow::anyhow!("低体力守门没有合法恢复动作"))?;
        }
        if !self.friend_outing_within_pacing(g) && a.get(c).is_some_and(|x| x.operation == Operation::FriendOuting) {
            // 配额约束的是所有友人外出，而不只是“替代休息”路径。
            c = out
                .iter()
                .enumerate()
                .filter(|(i, _)| a.get(*i).is_some_and(|x| x.operation != Operation::FriendOuting))
                .max_by(|(li, l), (ri, r)| l.score.total_cmp(&r.score).then_with(|| ri.cmp(li)))
                .map(|(i, _)| i)
                .ok_or_else(|| anyhow::anyhow!("友人外出达到跨年总配额后没有其他合法动作"))?;
        }
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
    /// 在不吃面的当前状态下返回真正会执行的基础动作。
    /// 用于在 RamenSelect 前决定本回合究竟是训练，还是应先休息/外出/治病/比赛。
    fn pre_eat_action(&self, g: &RamenGame) -> Result<Operation> {
        let mut preview = g.clone();
        preview.stage = RamenStage::Train;
        preview.ramen.current_ramen = None;
        preview.ramen.clear_pending();
        let actions = preview.list_actions()?;
        let (idx, _) = self.decide_train(&preview, &actions)?;
        actions
            .get(idx)
            .map(|a| a.operation)
            .ok_or_else(|| anyhow::anyhow!("吃面前训练决策索引越界: {idx}/{}", actions.len()))
    }

    /// 预演第三年某碗面落地后的最佳训练，返回 `(训练类型, 训练前体力, 训练后体力)`。
    ///
    /// 训练前体力回答“本回合是否应先恢复”，训练后体力回答“下一回合是否会崩盘”。
    /// 不落地随机分身，只使用当前可知面板与确定性拉面效果。
    /// 第三年本回合训练后，低体力是否还会伤害下一次普通训练。
    ///
    /// turn=70 后紧接 turn=71 有马纪念（赛后 +40），再进入 turn=72 超级拉面（回合开始 +20），
    /// 所以没有待保护的普通训练回合；此时体力归零也是合理终盘控制。
    fn y3_collapse_matters(&self, g: &RamenGame) -> bool {
        !self.config.y3_recovery_horizon || g.turn() < 70
    }

    fn post_ramen_vital_transition(&self, g: &RamenGame, region_id: usize) -> Result<Option<(usize, i32, i32)>> {
        if g.current_year() != 3 || g.turn() >= 72 {
            return Ok(None);
        }
        let mut preview = g.clone();
        preview.stage = RamenStage::Train;
        preview.ramen.current_ramen = Some(region_id);
        preview.ramen.clear_pending();
        let actions = preview.list_actions()?;
        let (idx, _) = self.decide_train(&preview, &actions)?;
        let Some(action) = actions.get(idx) else {
            anyhow::bail!("吃面后预演索引越界: {idx}/{}", actions.len());
        };
        let Operation::Train(tt) = action.operation else {
            return Ok(None);
        };
        let train = tt as usize;
        let buffs = preview.calc_training_buff(train)?;
        let value = preview.calc_training_value(&buffs, train)?;
        let before = preview.uma.vital;
        Ok(Some((train, before, before + value.vital)))
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

    fn deadline_urgency(&self, g: &RamenGame, post: i32) -> Result<f32> {
        if self.config.deadline_urgency_scale <= 0.0 {
            return Ok(0.0);
        }
        let year = (g.current_year() - 1) as usize;
        let data = RAMENDATA.get().ok_or_else(|| anyhow::anyhow!("RAMENDATA 未初始化"))?;
        let normal = *data.ramen_success_pt.get(year).unwrap_or(&i32::MAX);
        let target = if year == 2 { 5000 } else { normal };
        if post >= target {
            return Ok(0.0);
        }
        let turns = (Self::year_end(g) - g.turn() + 1).max(1) as f32;
        let gain = calc_ramen_pt_gain(year, g.ramen.eat_count + 1)?.max(1) as f32;
        let bowls_needed = ((target - post) as f32 / gain).ceil();
        let pressure = (bowls_needed / turns).clamp(0.0, 1.5);
        Ok(pressure * (target - post) as f32 * self.config.deadline_urgency_scale)
    }

    fn decide_special_dynamic(&self, g: &RamenGame, a: &[RamenAction]) -> Result<(usize, Vec<RamenPolicyOutput>)> {
        let (_, mut out) = self.policy.decide_special(g, a)?;
        for (act, score) in a.iter().zip(out.iter_mut()) {
            let Some(targets) = act.special_targets else { continue };
            let Some(region) = act.ramen else { continue };
            let mut post = g.ramen.clone();
            consume_for_ramen(&mut post, region, &targets)?;
            let craftable = g
                .ramen
                .selected_regions
                .iter()
                .filter(|&&rid| {
                    list_special_targets_for(&post, rid)
                        .map(|x| !x.is_empty())
                        .unwrap_or(false)
                })
                .count() as f32;
            let balance = post.feeling_stock.iter().map(|&x| (x as f32 + 2.0).sqrt()).sum::<f32>();
            let year_left = (Self::year_end(g) - g.turn()).max(0) as f32 / 21.0;
            let future = (craftable * 18.0 + balance * 4.0) * year_left;
            score.score += future;
            score.add("future_craftability", future);
            score.reason = format!("隐藏方案{:?} 后续可做{}种", targets, craftable as i32);
        }
        Ok((Self::choose(&out), out))
    }

    fn decide_ramen(&self, g: &RamenGame, a: &[RamenAction]) -> Result<(usize, Vec<RamenPolicyOutput>)> {
        let (_, mut out) = self.policy.decide_ramen(g, a)?;
        let pre_action = self.pre_eat_action(g)?;
        let year = (g.current_year() - 1) as usize;
        let eat_post = g.ramen.scenario_pt + calc_ramen_pt_gain(year, g.ramen.eat_count)?;
        let deadline_exception = self.deadline_urgency(g, eat_post)? > 0.0
            && matches!(pre_action, Operation::Race | Operation::Rest | Operation::FriendOuting);
        if self.config.eat_requires_training && !matches!(pre_action, Operation::Train(_)) && !deadline_exception {
            let no_eat = a
                .iter()
                .position(|action| action.ramen.is_none())
                .ok_or_else(|| anyhow::anyhow!("需要休息/外出时 RamenSelect 却没有不吃面候选"))?;
            for (i, candidate) in out.iter_mut().enumerate() {
                if i == no_eat {
                    candidate.reason = "不吃面：本回合基础决策不是训练".to_string();
                } else {
                    candidate.score = f32::NEG_INFINITY;
                    candidate.reason = "禁止吃面：本回合应先休息/外出/治病/比赛".to_string();
                }
            }
            return Ok((no_eat, out));
        }
        let risk = (g.ramen.feeling_stock.iter().sum::<i32>() - self.config.feeling_overflow_threshold).max(0) as f32;
        let bridge = self.safety_bridge(g, a)?;
        for (act, o) in a.iter().zip(out.iter_mut()) {
            if let Some(region_id) = act.ramen {
                if let Some((train, pre_vital, post_vital)) = self.post_ramen_vital_transition(g, region_id)? {
                    if train != 4
                        && self.config.y3_post_train_hard_floor > 0
                        && post_vital < self.config.y3_post_train_hard_floor
                    {
                        o.score = f32::NEG_INFINITY;
                        o.reason = format!(
                            "禁止吃面：第三年{}训练体力{}→{}低于硬底线{}",
                            ["速", "耐", "力", "根", "智"][train],
                            pre_vital,
                            post_vital,
                            self.config.y3_post_train_hard_floor
                        );
                        o.add("y3_vital_hard_guard", f32::NEG_INFINITY);
                        continue;
                    }
                    let pre_short = (self.config.y3_pre_train_vital_target - pre_vital).max(0) as f32;
                    let post_short = if self.y3_collapse_matters(g) {
                        (self.config.y3_post_train_vital_target - post_vital).max(0) as f32
                    } else {
                        0.0
                    };
                    let transition_cost = (pre_short + post_short) * self.config.y3_vital_shortfall_weight;
                    o.score -= transition_cost;
                    o.add(
                        "y3_pre_vital_shortfall",
                        -pre_short * self.config.y3_vital_shortfall_weight
                    );
                    o.add(
                        "y3_post_vital_shortfall",
                        -post_short * self.config.y3_vital_shortfall_weight
                    );
                }
                let pressure = risk * self.config.overflow_value;
                o.score += pressure;
                o.add("local_stock_pressure", pressure);
                let y = (g.current_year() - 1) as usize;
                let post = g.ramen.scenario_pt + calc_ramen_pt_gain(y, g.ramen.eat_count)?;
                let (ck, rmj, great) = self.scenario_threshold_value(g, post)?;
                let deadline = self.deadline_urgency(g, post)?;
                let window = self.ramen_window_alignment(g, region_id)?;
                let cook2_cost = self.cook2_ramen_stock_cost(g, region_id)?;
                let safety = if let Some((_, gain)) = bridge {
                    self.safety_bridge_candidate(g, region_id, gain)?
                } else {
                    0.0
                };
                let look = self.ramen_lookahead(g, region_id)?;
                o.score += ck + rmj + great + deadline + window + safety + look - cook2_cost;
                o.add("scenario_checkpoint", ck);
                o.add("rmj_cross", rmj);
                o.add("great_cross", great);
                o.add("deadline_urgency", deadline);
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

/// 当前经过配对基准验证的正式拉面杯手写策略。
///
/// 该类型把实验矩阵中表现最好的配置固化成一个可复用 preset，避免模拟器默认策略、
/// 蒙特卡洛 rollout 与 benchmark 各自复制参数后发生漂移。当前 preset 为：
///
/// - 分年技能 PT 权重：第一年 16，第二/三年 64；
/// - 长期结构最大即时分牺牲：140；
/// - 启用属性预留、动态体力、概率 Hint 与连续失败期望；
/// - 吃面 PT 权重：2.0；
/// - 当前真实训练窗口权重：0.10；
/// - 使用基础失败率作为保守决策风险预算（游戏规则仍应用真实减失败率）；
/// - Cook2 式诀窍边际库存权重：40；
/// - 关闭随机分身 lookahead；
/// - 第一/二年仅在体力低于 30 时硬休息，第三年取消硬休息门，改由连续评分决策；
/// - 吃面前先决定是否训练；吃面后强制从训练候选中选择，禁止休息浪费加成；
/// - 第三年终盘允许有马前把体力控到 0，随后由赛后 +40 与超级拉面每回合 +20 接管；
/// - 本来要休息时按 1/3/5 跨年累计节奏使用友人外出；即使万能材料暂时溢出也不禁止；
/// - 五段事件按当前体力、干劲、属性/PT及完链进度动态估值，第三段不再使用硬体力阈值；
/// - 不使用 RMJ 截止期紧迫度加分：300 局同种子矩阵中 deadline20/35/50 完全同轨，
///   平均分 56960.7，显著低于 deadline0 的 58881.6；硬目标仍由规则和既有跨线价值保证。
///
/// 这个结构只负责按年份转发给三份不可变策略；所有字段含义仍由
/// [`LocalRamenConfig`] 与 [`RamenPolicyConfig`] 的 Rustdoc 定义。
pub struct RecommendedRamenTrainer {
    years: [LocalRamenTrainer; 3],
    /// 最近一次调用落在哪一年的策略，用于把对应 breakdown 暴露给 LoggingTrainer。
    last_year: Mutex<Option<usize>>
}

impl RecommendedRamenTrainer {
    /// 构造当前正式推荐 preset。
    pub fn new() -> Self {
        fn make(pt_rate: f32, vital_rest: i32) -> LocalRamenTrainer {
            let mut policy = RamenPolicyConfig::default();
            policy.pt_rate = pt_rate;
            policy.ramen_pt_weight = 2.0;
            // 只在极低体力时保留下限；第三年彻底取消硬休息门，交给连续体力、
            // 失败期望与休息动作本身的分数比较，避免浪费终盘高价值训练回合。
            policy.vital_rest = vital_rest;
            // 保守风险预算：只影响策略打分，不改变规则层真实失败率。
            policy.effective_ramen_failure = false;

            let mut local = LocalRamenConfig::default();
            local.status_reserve_max = 40.0;
            local.dynamic_vital = true;
            local.probabilistic_hint = true;
            local.expected_fail = true;
            local.max_base_score_sacrifice = 140.0;
            local.ramen_window_weight = 0.10;
            local.ramen_lookahead_weight = 0.0;
            local.ramen_lookahead_samples = 1;
            local.effective_ramen_failure = false;
            local.cook2_stock_weight = 40.0;
            local.eat_requires_training = true;
            local.y3_pre_train_vital_target = 0;
            local.y3_post_train_vital_target = 0;
            local.y3_vital_shortfall_weight = 0.0;
            local.y3_post_train_hard_floor = 0;
            local.y3_recovery_horizon = true;
            local.friend_outing_replaces_rest = true;
            local.friend_outing3_recovery_vital = 0;
            local.friend_outing_cumulative_caps = [1, 3, 5];
            local.friend_rest_max_special = 4;
            local.deadline_urgency_scale = 0.0;
            local.dynamic_special_targets = true;
            LocalRamenTrainer::with_configs(policy, local)
        }

        Self {
            years: [make(16.0, 30), make(64.0, 30), make(64.0, 0)],
            last_year: Mutex::new(None)
        }
    }

    fn year(game: &RamenGame) -> usize {
        if game.turn() < 24 {
            0
        } else if game.turn() < 48 {
            1
        } else {
            2
        }
    }
}

impl Default for RecommendedRamenTrainer {
    fn default() -> Self {
        Self::new()
    }
}

impl Trainer<RamenGame> for RecommendedRamenTrainer {
    fn select_action(&self, game: &RamenGame, actions: &[RamenAction], rng: &mut StdRng) -> Result<usize> {
        let year = Self::year(game);
        if let Ok(mut slot) = self.last_year.lock() {
            *slot = Some(year);
        }
        self.years[year].select_action(game, actions, rng)
    }

    fn select_choice(&self, game: &RamenGame, choices: &[Vec<EventChoice>], rng: &mut StdRng) -> Result<usize> {
        let year = Self::year(game);
        if let Ok(mut slot) = self.last_year.lock() {
            *slot = Some(year);
        }
        self.years[year].select_choice(game, choices, rng)
    }

    fn select_event_choice(
        &self, game: &RamenGame, event: &EventData, choices: &[Vec<EventChoice>], rng: &mut StdRng
    ) -> Result<usize> {
        let year = Self::year(game);
        if let Ok(mut slot) = self.last_year.lock() {
            *slot = Some(year);
        }
        self.years[year].select_event_choice(game, event, choices, rng)
    }

    fn last_breakdown(&self) -> Option<String> {
        let year = (*self.last_year.lock().ok()?)?;
        self.years[year].last_breakdown()
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
            RamenStage::SpecialSelect => {
                if self.config.dynamic_special_targets {
                    self.decide_special_dynamic(g, a)?
                } else {
                    self.policy.decide_special(g, a)?
                }
            }
            RamenStage::RegionSelect => {
                let y = match g.turn() {
                    2 => 0,
                    23 => 1,
                    47 => 2,
                    _ => 0
                };
                self.policy.decide_region(g, y, a)?
            }
            _ => (0, Vec::new())
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
        &self, g: &RamenGame, e: &EventData, c: &[Vec<EventChoice>], r: &mut StdRng
    ) -> Result<usize> {
        if (830305111..=830305115).contains(&e.id) && !c.is_empty() {
            let (choice, _) = self.dynamic_friend_event_choice(g, c)?;
            return Ok(choice);
        }
        self.select_choice(g, c, r)
    }
    fn last_breakdown(&self) -> Option<String> {
        self.last_breakdown.lock().ok().and_then(|b| b.clone())
    }
}
