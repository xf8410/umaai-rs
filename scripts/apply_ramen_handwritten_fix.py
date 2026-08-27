#!/usr/bin/env python3
from pathlib import Path

path = Path("crates/umasim/src/trainer/local_ramen_trainer.rs")
text = path.read_text(encoding="utf-8")

replacements = [
    (
'''    fn decide_train(&self, g: &RamenGame, a: &[RamenAction]) -> Result<(usize, Vec<RamenPolicyOutput>)> {
        let (mut guard, mut out) = self.policy.decide_train(g, a)?;
''',
'''    fn decide_train(&self, g: &RamenGame, a: &[RamenAction]) -> Result<(usize, Vec<RamenPolicyOutput>)> {
        // 吃面后“必须训练”是候选集硬约束，不能只在基础策略触发休息/外出守门时补救。
        // 否则健康局面会走完整候选打分，休息、比赛和外出仍可能在吃面后胜出。
        let force_train = self.config.eat_requires_training && g.ramen.current_ramen.is_some();
        let (mut guard, mut out) = if force_train {
            let out = self.policy.score_train_actions(g, a)?;
            let guard = out
                .iter()
                .enumerate()
                .filter(|(i, _)| a.get(*i).is_some_and(|x| matches!(x.operation, Operation::Train(_))))
                .max_by(|(li, l), (ri, r)| l.score.total_cmp(&r.score).then_with(|| ri.cmp(li)))
                .map(|(i, _)| i)
                .ok_or_else(|| anyhow::anyhow!("已吃面但 Train 阶段没有训练候选"))?;
            (guard, out)
        } else {
            self.policy.decide_train(g, a)?
        };
'''
    ),
    (
'''        if out.len() != a.len() {
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
            let _ = out
                .iter()
                .enumerate()
                .filter(|(i, _)| a.get(*i).is_some_and(|x| matches!(x.operation, Operation::Train(_))))
                .max_by(|(li, l), (ri, r)| l.score.total_cmp(&r.score).then_with(|| ri.cmp(li)))
                .map(|(i, _)| i)
                .ok_or_else(|| anyhow::anyhow!("已吃面但 Train 阶段没有训练候选"))?;
        }
''',
'''        if out.len() != a.len() {
            // 未吃面时保留基础策略的生病/体力/心情/比赛守门。
            // 吃面后的 force_train 路径已在函数入口展开完整候选，因此不会进入这里。
            return Ok((guard, out));
        }
'''
    ),
    (
'''        let base = out.iter().map(|x| x.score).collect::<Vec<_>>();
        let bb = Self::choose(&out);
''',
'''        let base = out.iter().map(|x| x.score).collect::<Vec<_>>();
        let choose_allowed = |scores: &[RamenPolicyOutput]| -> Result<usize> {
            scores
                .iter()
                .enumerate()
                .filter(|(i, _)| {
                    !force_train
                        || a.get(*i).is_some_and(|x| matches!(x.operation, Operation::Train(_)))
                })
                .max_by(|(li, l), (ri, r)| l.score.total_cmp(&r.score).then_with(|| ri.cmp(li)))
                .map(|(i, _)| i)
                .ok_or_else(|| anyhow::anyhow!("当前候选集中没有合法动作"))
        };
        let bb = choose_allowed(&out)?;
'''
    ),
    (
'''        let lb = Self::choose(&out);
''',
'''        let lb = choose_allowed(&out)?;
'''
    ),
    (
'''            local.eat_requires_covered_train = true;
''',
'''            // “对应训练”只作为 coupling/weak boost 软权重；除吃后必训外，其余交给搜索。
            // 不再用 NEG_INFINITY 强制必须点 at_trains 覆盖位。
            local.eat_requires_covered_train = false;
'''
    ),
]

for old, new in replacements:
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"expected exactly one match, got {count}: {old[:100]!r}")
    text = text.replace(old, new)

marker = '''    /// 吃面-训练联动：当前吃面覆盖速位时，速训练候选获得显式 `ramen_train_coupling` 加分，
'''
test = r'''    /// 吃面后必训必须作用于完整候选路径：即使休息分数被抬到极高，最终也只能选训练。
    #[test]
    #[allow(clippy::panic)]
    fn eat_requires_training_filters_all_non_train_actions() -> Result<()> {
        use rand::{SeedableRng, prelude::StdRng};

        use crate::{
            game::{
                InheritInfo,
                ramen::{Operation, RamenGame, RamenStage}
            },
            gamedata::init_global,
            utils::{get_workspace_root, init_test_logger}
        };

        let workspace_root = get_workspace_root()?;
        std::env::set_current_dir(workspace_root)?;
        let _ = init_test_logger("error");
        let _ = init_global();

        let mut policy = RamenPolicyConfig::default();
        policy.vital_rest = 0;
        policy.vital_rest_eating = 0;
        policy.rest_base = 1_000_000.0;
        let mut local = LocalRamenConfig::default();
        local.eat_requires_training = true;
        let trainer = LocalRamenTrainer::with_configs(policy, local);
        let mut game = RamenGame::newgame(
            102601,
            &[302424, 302894, 303044, 302924, 303024, 303054],
            InheritInfo { blue_count: [15, 3, 0, 0, 0], extra_count: [0, 30, 0, 0, 30, 30] }
        )?;
        game.base.turn = 12;
        game.stage = RamenStage::Train;
        game.ramen.current_ramen = Some(0);
        let actions = game.list_actions()?;
        let mut rng = StdRng::seed_from_u64(42);
        let idx = trainer.select_action(&game, &actions, &mut rng)?;
        if !matches!(actions[idx].operation, Operation::Train(_)) {
            panic!("吃面后必须从五种训练中选择，实际为 {:?}", actions[idx].operation);
        }
        Ok(())
    }

    /// 正式 preset 只保留吃后必训硬约束；对应训练通过权重上浮，不使用覆盖位硬门。
    #[test]
    #[allow(clippy::panic)]
    fn recommended_uses_soft_covered_train_preference() {
        let trainer = RecommendedRamenTrainer::new();
        for (year, t) in trainer.years.each_ref().iter().enumerate() {
            if !t.config.eat_requires_training {
                panic!("year{year} 未启用吃面后必训");
            }
            if t.config.eat_requires_covered_train {
                panic!("year{year} 不应启用 at_trains 覆盖位硬门");
            }
            if t.config.ramen_train_coupling_weight <= 0.0 {
                panic!("year{year} 应通过 coupling 对对应训练做软加权");
            }
        }
    }

'''
if text.count(marker) != 1:
    raise SystemExit(f"test marker count is {text.count(marker)}")
text = text.replace(marker, test + marker)

path.write_text(text, encoding="utf-8")
