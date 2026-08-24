from pathlib import Path

p = Path('crates/umasim/src/trainer/local_ramen_trainer.rs')
s = p.read_text(encoding='utf-8')

def one(old, new):
    global s
    if s.count(old) != 1:
        raise SystemExit(f'expected one match, got {s.count(old)}: {old[:120]!r}')
    s = s.replace(old, new)

one(
'''    pub eat_requires_training: bool,

    /// 第三年吃面前希望具备的训练前体力''',
'''    pub eat_requires_training: bool,

    /// 已经吃面后是否强制从训练候选中选择。
    ///
    /// 该字段独立于吃面前检查，仅用于严格配对消融；当前正式策略保持 `true`，
    /// `nopostforce` 实验 token 可单独关闭。
    pub post_eat_force_training: bool,

    /// 第三年吃面前希望具备的训练前体力''')

one(
'''            eat_requires_training: false,
            y3_pre_train_vital_target: 0,''',
'''            eat_requires_training: false,
            post_eat_force_training: false,
            y3_pre_train_vital_target: 0,''')

one(
'''            } else if token == "eatguard" {
                local.eat_requires_training = true
            } else if let Some(v) = token.strip_prefix("y3pre") {''',
'''            } else if token == "eatguard" {
                local.eat_requires_training = true;
                local.post_eat_force_training = true
            } else if token == "nopostforce" {
                local.post_eat_force_training = false
            } else if let Some(v) = token.strip_prefix("y3pre") {''')

one(
'''            let ate_this_turn = self.config.eat_requires_training && g.ramen.current_ramen.is_some();''',
'''            let ate_this_turn = self.config.post_eat_force_training && g.ramen.current_ramen.is_some();''')

one(
'''            local.eat_requires_training = true;
            local.y3_pre_train_vital_target = 0;''',
'''            local.eat_requires_training = true;
            local.post_eat_force_training = true;
            local.y3_pre_train_vital_target = 0;''')

p.write_text(s, encoding='utf-8')

p = Path('crates/umasim/tools/data_collection/skill_pt_phase_matrix.rs')
s = p.read_text(encoding='utf-8')

def one2(old, new):
    global s
    if s.count(old) != 1:
        raise SystemExit(f'expected one matrix match, got {s.count(old)}: {old[:120]!r}')
    s = s.replace(old, new)

one2(
'''    trainer::{LocalRamenTrainer, LoggingTrainer, RamenHandwrittenTrainer},''',
'''    trainer::{LocalRamenTrainer, LoggingTrainer},''')

one2(
'''    fn new(pt: [u32; 3], sac: u32, common: &str, yearly: [&str; 3]) -> Result<Self> {''',
'''    fn new(pt: [u32; 3], sac: u32, common: &str, yearly: [&str; 3], suffix: &str) -> Result<Self> {''')

one2(
'''            if !yearly[year].is_empty() {
                tokens.push(yearly[year].to_string());
            }
            LocalRamenTrainer::matrix_variant(&tokens.join("-"))''',
'''            if !yearly[year].is_empty() {
                tokens.push(yearly[year].to_string());
            }
            if !suffix.is_empty() {
                tokens.push(suffix.to_string());
            }
            LocalRamenTrainer::matrix_variant(&tokens.join("-"))''')

one2(
'''fn run<T: Trainer<RamenGame>>(t: T, i: u64) -> Result<bench::GameOutcome> {''',
'''fn run<T: Trainer<RamenGame>>(t: T, i: u64) -> Result<bench::GameOutcome> {''')

one2(
'''    let validation = PhaseTrainer::new(pt, sac, &extra, yearly_refs).context("分阶段策略参数验证失败")?;''',
'''    let validation = PhaseTrainer::new(pt, sac, &extra, yearly_refs, "").context("分阶段策略参数验证失败")?;''')

one2(
'''        let a = run(RamenHandwrittenTrainer::new(), i)?;
        let b = run(PhaseTrainer::new(pt, sac, &extra, yearly_refs)?, i)?;''',
'''        let a = run(PhaseTrainer::new(pt, sac, &extra, yearly_refs, "")?, i)?;
        let b = run(PhaseTrainer::new(pt, sac, &extra, yearly_refs, "nopostforce")?, i)?;''')

p.write_text(s, encoding='utf-8')
