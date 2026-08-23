from pathlib import Path

p = Path("crates/umasim/src/trainer/local_ramen_trainer.rs")
s = p.read_text()
start = s.index("pub struct LocalRamenConfig {")
end = s.index("\n}\nimpl Default for LocalRamenConfig", start) + 2
new = '''pub struct LocalRamenConfig {
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
}'''
s = s[:start] + new + s[end:]
p.write_text(s)
