from pathlib import Path
p=Path('crates/umasim/src/trainer/local_ramen_trainer.rs')
s=p.read_text()

# Remove the overflow hard gate: the +2 material source remains strategically required even when
# the current counter is full; yearly total caps, not current stock, pace the finite outings.
s=s.replace('''            && self.friend_outing_within_pacing(g)
            && g.ramen.special_feeling <= self.config.friend_rest_max_special
            && a.get(guard).is_some_and(|x| x.operation == Operation::Rest)
''','''            && self.friend_outing_within_pacing(g)
            && a.get(guard).is_some_and(|x| x.operation == Operation::Rest)
''',1)

# Add state-aware valuation helpers immediately before decide_train.
anchor='''    fn decide_train(&self, g: &RamenGame, a: &[RamenAction]) -> Result<(usize, Vec<RamenPolicyOutput>)> {
'''
helper='''    /// 下一段友人外出的动态价值。
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
            ),
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
                adjust += c.value.max_vital as f32 * self.policy.config.event_vital_weight * prob;
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

'''
if s.count(anchor)!=1: raise SystemExit(f'helper anchor count={s.count(anchor)}')
s=s.replace(anchor,helper+anchor)

# Once a full candidate table exists, replace the old fixed 15+45 score with dynamic components.
anchor='''        let base = out.iter().map(|x| x.score).collect::<Vec<_>>();
'''
insert='''        if let Some(friend_idx) = a.iter().position(|x| x.operation == Operation::FriendOuting) {
            let (score, breakdown, reason) = self.dynamic_friend_outing_value(g)?;
            if let Some(friend) = out.get_mut(friend_idx) {
                friend.score = score;
                friend.breakdown = breakdown;
                friend.reason = reason;
            }
        }
        let base = out.iter().map(|x| x.score).collect::<Vec<_>>();
'''
if s.count(anchor)!=1: raise SystemExit(f'candidate anchor count={s.count(anchor)}')
s=s.replace(anchor,insert,1)

# Event 3 and any future selectable outing use the same dynamic clipping rather than a hard vital threshold.
old='''        if e.id == 830305113
            && self.config.friend_outing3_recovery_vital > 0
            && g.uma.vital < self.config.friend_outing3_recovery_vital
            && !c.is_empty()
        {
            // 友人外出3选项1固定恢复50体；在低体力恢复场景中不能被高PT权重误选成无回复选项。
            return Ok(0);
        }
        self.select_choice(g, c, r)
'''
new='''        if (830305111..=830305115).contains(&e.id) && !c.is_empty() {
            let (choice, _) = self.dynamic_friend_event_choice(g, c)?;
            return Ok(choice);
        }
        self.select_choice(g, c, r)
'''
if s.count(old)!=1: raise SystemExit(f'event override count={s.count(old)}')
s=s.replace(old,new,1)

# Production pacing and comments: keep required outing behaviour but pace 1/3/5 across years.
s=s.replace('''            local.friend_outing3_recovery_vital = 45;
''','''            local.friend_outing3_recovery_vital = 0;
            local.friend_outing_cumulative_caps = [1, 3, 5];
            local.friend_rest_max_special = 4;
''',1)
s=s.replace('''/// - 本来要休息且友人外出可用时，以友人外出替代纯休息；第三次外出低于 45 体力时选择回 50 体。
''','''/// - 本来要休息时按 1/3/5 跨年累计节奏使用友人外出；即使万能材料暂时溢出也不禁止；
/// - 五段事件按当前体力、干劲、属性/PT及完链进度动态估值，第三段不再使用硬体力阈值。
''',1)

p.write_text(s)
