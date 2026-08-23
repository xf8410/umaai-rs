from pathlib import Path

# Add a stable public preset wrapper around the three yearly LocalRamenTrainer policies.
p = Path("crates/umasim/src/trainer/local_ramen_trainer.rs")
s = p.read_text()
anchor = "impl Trainer<RamenGame> for LocalRamenTrainer {"
if anchor not in s:
    raise SystemExit("LocalRamenTrainer impl anchor missing")
insert = r'''
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
/// - 关闭随机分身 lookahead。
///
/// 这个结构只负责按年份转发给三份不可变策略；所有字段含义仍由
/// [`LocalRamenConfig`] 与 [`RamenPolicyConfig`] 的 Rustdoc 定义。
pub struct RecommendedRamenTrainer {
    years: [LocalRamenTrainer; 3],
    /// 最近一次调用落在哪一年的策略，用于把对应 breakdown 暴露给 LoggingTrainer。
    last_year: Mutex<Option<usize>>,
}

impl RecommendedRamenTrainer {
    /// 构造当前正式推荐 preset。
    pub fn new() -> Self {
        fn make(pt_rate: f32) -> LocalRamenTrainer {
            let mut policy = RamenPolicyConfig::default();
            policy.pt_rate = pt_rate;
            policy.ramen_pt_weight = 2.0;
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
            LocalRamenTrainer::with_configs(policy, local)
        }

        Self {
            years: [make(16.0), make(64.0), make(64.0)],
            last_year: Mutex::new(None),
        }
    }

    fn year(game: &RamenGame) -> usize {
        if game.turn() < 24 { 0 } else if game.turn() < 48 { 1 } else { 2 }
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
        &self,
        game: &RamenGame,
        event: &EventData,
        choices: &[Vec<EventChoice>],
        rng: &mut StdRng,
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

'''
s = s.replace(anchor, insert + anchor, 1)
p.write_text(s)

# Export the preset from trainer module.
p = Path("crates/umasim/src/trainer/mod.rs")
s = p.read_text()
old = "pub use local_ramen_trainer::LocalRamenTrainer;"
new = "pub use local_ramen_trainer::{LocalRamenTrainer, RecommendedRamenTrainer};"
if s.count(old) != 1:
    raise SystemExit(f"trainer export count={s.count(old)}")
p.write_text(s.replace(old, new))

# Make full-depth ramen Monte Carlo rollouts use the same best-known production baseline.
p = Path("crates/umasim/src/search/searchable.rs")
s = p.read_text()
old = "type RolloutTrainer = crate::trainer::RamenHandwrittenTrainer;"
new = "type RolloutTrainer = crate::trainer::RecommendedRamenTrainer;"
if s.count(old) != 1:
    raise SystemExit(f"rollout associated type count={s.count(old)}")
s = s.replace(old, new)
old = "crate::trainer::RamenHandwrittenTrainer::new()"
new = "crate::trainer::RecommendedRamenTrainer::new()"
if s.count(old) != 1:
    raise SystemExit(f"rollout constructor count={s.count(old)}")
s = s.replace(old, new)
p.write_text(s)

# Wire normal CLI handwritten ramen runs to the same preset.
p = Path("crates/umasim/src/main.rs")
s = p.read_text()
old = "let trainer = RamenHandwrittenTrainer::new();"
new = "let trainer = RecommendedRamenTrainer::new();"
if s.count(old) != 1:
    raise SystemExit(f"CLI handwritten constructor count={s.count(old)}")
p.write_text(s.replace(old, new))
