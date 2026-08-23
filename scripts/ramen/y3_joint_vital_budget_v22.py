from pathlib import Path
p=Path('crates/umasim/src/trainer/local_ramen_trainer.rs')
s=p.read_text()

# Replace the single hard post floor with a joint transition budget.
old='''    /// 第三年吃面前要求“吃面后所选训练”结束时至少保留的体力。
    ///
    /// 仅在普通第三年回合（48-71）生效。策略会针对每碗候选面预演吃面后的最佳训练；
    /// 若非智力训练结束后的体力低于该值，就禁止该面，避免虽然本回合因 100% 减失败率
    /// 能训练，却把体力打空并导致下一回合被迫恢复。智力训练会回体力，不受此门控。
    /// `0` 表示关闭。
    pub y3_post_train_vital_floor: i32,
'''
new='''    /// 第三年吃面前希望具备的训练前体力，单位为体力点。
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
'''
if s.count(old)!=1: raise SystemExit(f'old config docs {s.count(old)}')
s=s.replace(old,new)
s=s.replace('''            y3_post_train_vital_floor: 0,
''','''            y3_pre_train_vital_target: 0,
            y3_post_train_vital_target: 0,
            y3_vital_shortfall_weight: 0.0,
            y3_post_train_hard_floor: 0,
''',1)

# Replace parser token.
old='''            } else if let Some(v) = token.strip_prefix("y3floor") {
                local.y3_post_train_vital_floor = v.parse()?
            } else if token == "failmodel" {
'''
new='''            } else if let Some(v) = token.strip_prefix("y3pre") {
                local.y3_pre_train_vital_target = v.parse()?
            } else if let Some(v) = token.strip_prefix("y3post") {
                local.y3_post_train_vital_target = v.parse()?
            } else if let Some(v) = token.strip_prefix("y3vw") {
                local.y3_vital_shortfall_weight = v.parse()?
            } else if let Some(v) = token.strip_prefix("y3hard") {
                local.y3_post_train_hard_floor = v.parse()?
            } else if token == "failmodel" {
'''
if s.count(old)!=1: raise SystemExit(f'parser {s.count(old)}')
s=s.replace(old,new)

# Production preset: undo v21's harmful hard floor; use a modest joint soft budget and only an
# extreme non-wisdom hard floor.
s=s.replace('''            local.y3_post_train_vital_floor = 20;
''','''            local.y3_pre_train_vital_target = 30;
            local.y3_post_train_vital_target = 10;
            local.y3_vital_shortfall_weight = 8.0;
            local.y3_post_train_hard_floor = 0;
''',1)
s=s.replace('''/// - 第三年逐碗预演吃面后的训练，非智力训练后至少保留 20 体力，保护下一回合。
''','''/// - 第三年逐碗预演训练前后体力，以软成本联合评价 `V0` 与 `V1`，避免单端硬门过度保守。
''',1)

# Replace helper; now return train and delta endpoints even when feature targets are disabled.
start=s.index('''    /// 预演第三年某碗面落地后的最佳训练，返回 `(训练类型, 训练后体力)`。''')
end=s.index('''    fn best_action_score(&self, g: &RamenGame) -> Result<f32> {''', start)
helper='''    /// 预演第三年某碗面落地后的最佳训练，返回 `(训练类型, 训练前体力, 训练后体力)`。
    ///
    /// 训练前体力回答“本回合是否应先恢复”，训练后体力回答“下一回合是否会崩盘”。
    /// 不落地随机分身，只使用当前可知面板与确定性拉面效果。
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
        let Operation::Train(tt) = action.operation else { return Ok(None) };
        let train = tt as usize;
        let buffs = preview.calc_training_buff(train)?;
        let value = preview.calc_training_value(&buffs, train)?;
        let before = preview.uma.vital;
        Ok(Some((train, before, before + value.vital)))
    }

'''
s=s[:start]+helper+s[end:]

# Replace candidate hard gate with joint soft accounting + extreme hard floor.
old='''                if let Some((train, post_vital)) = self.post_ramen_training_vital(g, region_id)? {
                    let wisdom_recovers = train == 4;
                    if !wisdom_recovers && post_vital < self.config.y3_post_train_vital_floor {
                        o.score = f32::NEG_INFINITY;
                        o.reason = format!(
                            "禁止吃面：第三年预演{}训练后体力{}<保留{}",
                            ["速", "耐", "力", "根", "智"][train],
                            post_vital,
                            self.config.y3_post_train_vital_floor
                        );
                        o.add("y3_next_turn_vital_guard", f32::NEG_INFINITY);
                        continue;
                    }
                }
'''
new='''                if let Some((train, pre_vital, post_vital)) = self.post_ramen_vital_transition(g, region_id)? {
                    if train != 4
                        && self.config.y3_post_train_hard_floor > 0
                        && post_vital < self.config.y3_post_train_hard_floor
                    {
                        o.score = f32::NEG_INFINITY;
                        o.reason = format!(
                            "禁止吃面：第三年{}训练体力{}→{}低于硬底线{}",
                            ["速", "耐", "力", "根", "智"][train], pre_vital, post_vital,
                            self.config.y3_post_train_hard_floor
                        );
                        o.add("y3_vital_hard_guard", f32::NEG_INFINITY);
                        continue;
                    }
                    let pre_short = (self.config.y3_pre_train_vital_target - pre_vital).max(0) as f32;
                    let post_short = (self.config.y3_post_train_vital_target - post_vital).max(0) as f32;
                    let transition_cost = (pre_short + post_short) * self.config.y3_vital_shortfall_weight;
                    o.score -= transition_cost;
                    o.add("y3_pre_vital_shortfall", -pre_short * self.config.y3_vital_shortfall_weight);
                    o.add("y3_post_vital_shortfall", -post_short * self.config.y3_vital_shortfall_weight);
                }
'''
if s.count(old)!=1: raise SystemExit(f'candidate old {s.count(old)}')
s=s.replace(old,new)
p.write_text(s)
