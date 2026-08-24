from pathlib import Path
import runpy

# Reuse the validated strict PhaseTrainer A/B suffix patch from v43.
runpy.run_path('scripts/ramen/friend_cap_matrix_v43.py', run_name='__main__')

p = Path('crates/umasim/src/trainer/local_ramen_trainer.rs')
s = p.read_text(encoding='utf-8')

def one(old, new):
    global s
    if s.count(old) != 1:
        raise SystemExit(f'expected one match, got {s.count(old)}: {old[:140]!r}')
    s = s.replace(old, new)

one(
'''    /// RMJ/第三年5000目标在截止前的可达性紧迫度。
    pub deadline_urgency_scale: f32,''',
'''    /// RMJ/第三年5000目标在截止前的可达性紧迫度。
    pub deadline_urgency_scale: f32,

    /// 基础动作不是训练时，是否允许“当前这一碗确定首次跨过年度 RMJ 线”的候选继续比较。
    pub exact_rmj_cross_bypass: bool,

    /// 是否允许第三年“当前这一碗确定首次跨过 5000 大成功线”的候选绕过吃面前恢复门。
    pub exact_great_cross_bypass: bool,

    /// 是否允许“当前这一碗确定提高 PT 常驻效果阶梯”的候选绕过吃面前恢复门。
    pub exact_effect_step_bypass: bool,

    /// 确定跨线放行是否要求吃后至少保留一种当前已选地区的可制作拉面。
    pub exact_cross_keep_craftable: bool,''')

one(
'''            friend_rest_max_special: 4,
            deadline_urgency_scale: 0.0,
            dynamic_special_targets: false''',
'''            friend_rest_max_special: 4,
            deadline_urgency_scale: 0.0,
            exact_rmj_cross_bypass: false,
            exact_great_cross_bypass: false,
            exact_effect_step_bypass: false,
            exact_cross_keep_craftable: false,
            dynamic_special_targets: false''')

one(
'''            } else if let Some(v) = token.strip_prefix("deadline") {
                local.deadline_urgency_scale = v.parse::<f32>()? / 100.0
            } else if token == "specialdynamic" {''',
'''            } else if let Some(v) = token.strip_prefix("deadline") {
                local.deadline_urgency_scale = v.parse::<f32>()? / 100.0
            } else if token == "crossrmj" {
                local.exact_rmj_cross_bypass = true
            } else if token == "crossgreat" {
                local.exact_great_cross_bypass = true
            } else if token == "crossstep" {
                local.exact_effect_step_bypass = true
            } else if token == "crosskeep" {
                local.exact_cross_keep_craftable = true
            } else if token == "specialdynamic" {''')

one(
'''    fn decide_ramen(&self, g: &RamenGame, a: &[RamenAction]) -> Result<(usize, Vec<RamenPolicyOutput>)> {''',
'''    fn exact_progression_bypass(&self, g: &RamenGame, region_id: usize, post: i32) -> Result<bool> {
        let year = (g.current_year() - 1).clamp(0, 2) as usize;
        let data = RAMENDATA.get().ok_or_else(|| anyhow::anyhow!("RAMENDATA 未初始化"))?;
        let rmj_target = *data.ramen_success_pt.get(year).unwrap_or(&i32::MAX);
        let rmj = self.config.exact_rmj_cross_bypass && g.ramen.scenario_pt < rmj_target && post >= rmj_target;
        let great = self.config.exact_great_cross_bypass
            && year == 2
            && g.ramen.scenario_pt < 5000
            && post >= 5000;
        let before_effect = Self::pt_effect(g.ramen.scenario_pt)?;
        let after_effect = Self::pt_effect(post)?;
        let region_step = calc_region_bonus(post) > calc_region_bonus(g.ramen.scenario_pt);
        let step = self.config.exact_effect_step_bypass && (after_effect > before_effect || region_step);
        if !(rmj || great || step) {
            return Ok(false);
        }
        if !self.config.exact_cross_keep_craftable {
            return Ok(true);
        }
        let targets = list_special_targets_for(&g.ramen, region_id)?
            .into_iter()
            .min_by_key(|x| x.iter().sum::<i32>())
            .ok_or_else(|| anyhow::anyhow!("跨线候选 {region_id} 没有合法材料方案"))?;
        let mut post_stock = g.ramen.clone();
        consume_for_ramen(&mut post_stock, region_id, &targets)?;
        Ok(post_stock.selected_regions.iter().any(|&rid| {
            list_special_targets_for(&post_stock, rid).map(|x| !x.is_empty()).unwrap_or(false)
        }))
    }

    fn decide_ramen(&self, g: &RamenGame, a: &[RamenAction]) -> Result<(usize, Vec<RamenPolicyOutput>)> {''')

old = '''        let deadline_exception = self.deadline_urgency(g, eat_post)? > 0.0
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
        }'''
new = '''        let needs_recovery = self.config.eat_requires_training && !matches!(pre_action, Operation::Train(_));
        if needs_recovery {
            let mut any_bypass = false;
            for (act, candidate) in a.iter().zip(out.iter_mut()) {
                let allowed = match act.ramen {
                    Some(region_id) => self.exact_progression_bypass(g, region_id, eat_post)?,
                    None => true
                };
                if act.ramen.is_some() && allowed {
                    any_bypass = true;
                    candidate.reason = format!("确定跨线放行；{}", candidate.reason);
                } else if !allowed {
                    candidate.score = f32::NEG_INFINITY;
                    candidate.reason = "禁止吃面：本回合应先恢复/比赛且当前一碗不确定跨线".to_string();
                }
            }
            if !any_bypass {
                let no_eat = a
                    .iter()
                    .position(|action| action.ramen.is_none())
                    .ok_or_else(|| anyhow::anyhow!("需要恢复/比赛时 RamenSelect 却没有不吃面候选"))?;
                return Ok((no_eat, out));
            }
        }'''
one(old, new)

one(
'''            local.deadline_urgency_scale = 0.0;
            local.dynamic_special_targets = true;''',
'''            local.deadline_urgency_scale = 0.0;
            local.exact_rmj_cross_bypass = false;
            local.exact_great_cross_bypass = false;
            local.exact_effect_step_bypass = false;
            local.exact_cross_keep_craftable = false;
            local.dynamic_special_targets = true;''')

p.write_text(s, encoding='utf-8')
