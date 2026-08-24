from pathlib import Path

p = Path('crates/umasim/src/trainer/local_ramen_trainer.rs')
s = p.read_text(encoding='utf-8')

def one(old, new):
    global s
    if s.count(old) != 1:
        raise SystemExit(f'expected exactly one match, got {s.count(old)}: {old[:120]!r}')
    s = s.replace(old, new)

one(
'''    /// 是否把“吃面”和“本回合训练”视为不可拆分的事务。
    ///
    /// `true` 时先在不吃面的当前局面决定基础动作：若应休息、外出、治病或比赛，
    /// RamenSelect 直接选择不吃；一旦已经吃面，Train 阶段只在五种训练中比较，
    /// 不允许随后休息而浪费仅本回合生效的拉面加成。
    pub eat_requires_training: bool,''',
'''    /// 吃面前是否先确认本回合的基础动作是训练。
    ///
    /// `true` 时在不吃面的当前局面预演一次动作；若应休息、外出、治病或比赛，
    /// RamenSelect 默认选择不吃。该检查只负责避免事前浪费材料；一旦已经吃面，
    /// Train 阶段仍比较全部合法动作，不因沉没成本强制训练。
    pub pre_eat_training_check: bool,''')

one('''            eat_requires_training: false,''', '''            pre_eat_training_check: false,''')
one('''                local.eat_requires_training = true''', '''                // 兼容历史矩阵 token；现在只启用吃面前训练检查，不再强制吃面后训练。
                local.pre_eat_training_check = true''')

old = '''        if out.len() != a.len() {
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
        }'''
new = '''        if out.len() != a.len() {
            // 守门结论对吃面前后使用相同语义。已经支付的材料是沉没成本，不能成为
            // 强制训练的理由；是否应在本回合吃面由 RamenSelect 的事前检查负责。
            return Ok((guard, out));
        }'''
one(old, new)

one(
'''        if self.config.eat_requires_training && !matches!(pre_action, Operation::Train(_)) && !deadline_exception {''',
'''        if self.config.pre_eat_training_check && !matches!(pre_action, Operation::Train(_)) && !deadline_exception {''')

one('''            local.eat_requires_training = true;''', '''            local.pre_eat_training_check = true;''')

one(
'''/// - 吃面前先决定是否训练；吃面后强制从训练候选中选择，禁止休息浪费加成；''',
'''/// - 吃面前先决定是否训练；吃面后重新比较全部合法动作，不追逐已支付的沉没成本；''')

if 'eat_requires_training' in s:
    raise SystemExit('stale eat_requires_training reference remains')
p.write_text(s, encoding='utf-8')
