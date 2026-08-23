from pathlib import Path
p=Path('crates/umasim/src/trainer/local_ramen_trainer.rs')
s=p.read_text()

# Config switch, documented because benchmark ablation needs an explicit control.
anchor='''    pub cook2_stock_weight: f32,
}'''
replace='''    pub cook2_stock_weight: f32,

    /// 是否把“吃面”和“本回合训练”视为不可拆分的事务。
    ///
    /// `true` 时先在不吃面的当前局面决定基础动作：若应休息、外出、治病或比赛，
    /// RamenSelect 直接选择不吃；一旦已经吃面，Train 阶段只在五种训练中比较，
    /// 不允许随后休息而浪费仅本回合生效的拉面加成。
    pub eat_requires_training: bool,
}'''
if s.count(anchor)!=1: raise SystemExit(f'config anchor {s.count(anchor)}')
s=s.replace(anchor,replace)
s=s.replace('''            cook2_stock_weight: 0.0,
''','''            cook2_stock_weight: 0.0,
            eat_requires_training: false,
''',1)

# Parser allows exact A/B while recommended preset enables it.
anchor='''            } else if let Some(v) = token.strip_prefix("vrest") {
                policy.vital_rest = v.parse()?
            } else if token == "failmodel" {
'''
replace='''            } else if let Some(v) = token.strip_prefix("vrest") {
                policy.vital_rest = v.parse()?
            } else if token == "eatguard" {
                local.eat_requires_training = true
            } else if token == "failmodel" {
'''
if s.count(anchor)!=1: raise SystemExit(f'parser anchor {s.count(anchor)}')
s=s.replace(anchor,replace)
s=s.replace('''            local.cook2_stock_weight = 40.0;
''','''            local.cook2_stock_weight = 40.0;
            local.eat_requires_training = true;
''',1)
s=s.replace('''/// - 第一/二年仅在体力低于 30 时硬休息，第三年取消硬休息门，改由连续评分决策。
''','''/// - 第一/二年仅在体力低于 30 时硬休息，第三年取消硬休息门，改由连续评分决策；
/// - 吃面前先决定是否训练；吃面后强制从训练候选中选择，禁止休息浪费加成。
''',1)

# Helper: inspect the actual pre-eat action, not only its score.
anchor='''    fn best_action_score(&self, g: &RamenGame) -> Result<f32> {
'''
helper='''    /// 在不吃面的当前状态下返回真正会执行的基础动作。
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

'''
if s.count(anchor)!=1: raise SystemExit(f'helper anchor {s.count(anchor)}')
s=s.replace(anchor,helper+anchor)

# After eating: keep illness/free-race guards in canonical policy, but if it returns a non-training
# operation, rescore and select only train candidates. This is a defensive invariant; normally the
# pre-eat gate prevents reaching an incompatible state.
anchor='''        let (guard, mut out) = self.policy.decide_train(g, a)?;
        if out.len() != a.len() {
            return Ok((guard, out));
        }
'''
replace='''        let (mut guard, mut out) = self.policy.decide_train(g, a)?;
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
'''
if s.count(anchor)!=1: raise SystemExit(f'decide train anchor {s.count(anchor)}')
s=s.replace(anchor,replace)

# RamenSelect: if the actual no-eat decision is not training, force no ramen. Do this before adding
# ramen candidate bonuses, so no threshold/stock factor can accidentally override the invariant.
anchor='''    fn decide_ramen(&self, g: &RamenGame, a: &[RamenAction]) -> Result<(usize, Vec<RamenPolicyOutput>)> {
        let (_, mut out) = self.policy.decide_ramen(g, a)?;
'''
replace='''    fn decide_ramen(&self, g: &RamenGame, a: &[RamenAction]) -> Result<(usize, Vec<RamenPolicyOutput>)> {
        let (_, mut out) = self.policy.decide_ramen(g, a)?;
        if self.config.eat_requires_training && !matches!(self.pre_eat_action(g)?, Operation::Train(_)) {
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
'''
if s.count(anchor)!=1: raise SystemExit(f'decide ramen anchor {s.count(anchor)}')
s=s.replace(anchor,replace)

p.write_text(s)
