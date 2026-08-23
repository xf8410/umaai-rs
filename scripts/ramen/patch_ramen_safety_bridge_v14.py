from pathlib import Path
p=Path('crates/umasim/src/trainer/local_ramen_trainer.rs')
s=p.read_text()
s=s.replace(
'''            rules::{calc_ramen_pt_gain, calc_region_bonus, list_special_targets_for},''',
'''            rules::{calc_ramen_pt_gain, calc_region_bonus, consume_for_ramen, list_special_targets_for},''')
s=s.replace(
'''    pub effective_ramen_failure: bool,
''',
'''    pub effective_ramen_failure: bool,
    /// First-year-only safety bridge: minimum raw failure rate of the rescued training.
    pub safety_bridge_min_fail: f32,
    /// Minimum score improvement required after applying Y1's 30% relative failure reduction.
    pub safety_bridge_min_gain: f32,
    /// Cost per lost post-eat craftable option and per hidden flavor consumed.
    pub safety_bridge_stock_cost: f32,
''')
s=s.replace(
'''            effective_ramen_failure: true,
''',
'''            effective_ramen_failure: true,
            safety_bridge_min_fail: 101.0,
            safety_bridge_min_gain: 0.0,
            safety_bridge_stock_cost: 0.0,
''')
s=s.replace(
'''            } else if token == "failmodel" {
''',
'''            } else if let Some(v) = token.strip_prefix("bridge") {
                local.safety_bridge_min_fail = v.parse()?
            } else if let Some(v) = token.strip_prefix("bgain") {
                local.safety_bridge_min_gain = v.parse()?
            } else if let Some(v) = token.strip_prefix("bcost") {
                local.safety_bridge_stock_cost = v.parse()?
            } else if token == "failmodel" {
''')
anchor='''    fn decide_ramen(&self, g: &RamenGame, a: &[RamenAction]) -> Result<(usize, Vec<RamenPolicyOutput>)> {
'''
method='''    /// Detect a narrow Y1 safety transition. The normal train policy stays conservative
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
        if outs.len() != actions.len() { return Ok(None); }
        let raw_best = outs.iter().map(|x| x.score).fold(f32::NEG_INFINITY, f32::max);
        let mut rescued: Option<(usize, f32)> = None;
        for (act, out) in actions.iter().zip(outs.iter()) {
            let Operation::Train(tt) = act.operation else { continue };
            let tr = tt as usize;
            let buffs = preview.calc_training_buff(tr)?;
            let fr = preview.calc_training_failure_rate(&buffs, tr);
            if fr < self.config.safety_bridge_min_fail { continue; }
            let fail_adj = out.breakdown.iter().find(|(k, _)| k == "fail_adj").map(|(_, v)| *v).unwrap_or(0.0);
            let gross = out.score - fail_adj;
            let effective_fr = fr * 0.70;
            let effective_adj = -(gross * effective_fr / 100.0
                + self.policy.config.failure_penalty * effective_fr / 100.0);
            let effective_score = gross + effective_adj;
            let gain = effective_score - raw_best;
            if gain >= self.config.safety_bridge_min_gain
                && rescued.map(|(_, old)| gain > old).unwrap_or(true) {
                rescued = Some((tr, gain));
            }
        }
        let Some((tr, gain)) = rescued else { return Ok(None); };
        let d = RAMENDATA.get().ok_or_else(|| anyhow::anyhow!("RAMENDATA 未初始化"))?;
        let covered = ramen_actions.iter().filter_map(|x| x.ramen).any(|rid| {
            d.ramen_region_effect.get(rid).map(|r| r.at_trains.contains(&(tr as i32))).unwrap_or(false)
        });
        Ok(if covered { None } else { Some((tr, gain)) })
    }

    fn safety_bridge_candidate(&self, g: &RamenGame, region_id: usize, gain: f32) -> Result<f32> {
        let targets = list_special_targets_for(&g.ramen, region_id)?
            .into_iter().min_by_key(|t| t.iter().sum::<i32>())
            .ok_or_else(|| anyhow::anyhow!("拉面 {region_id} 无合法 targets"))?;
        let used = targets.iter().sum::<i32>() as f32;
        let before = g.ramen.selected_regions.iter()
            .filter(|&&rid| list_special_targets_for(&g.ramen, rid).map(|x| !x.is_empty()).unwrap_or(false)).count();
        let mut post = g.ramen.clone();
        consume_for_ramen(&mut post, region_id, &targets)?;
        let after = g.ramen.selected_regions.iter()
            .filter(|&&rid| list_special_targets_for(&post, rid).map(|x| !x.is_empty()).unwrap_or(false)).count();
        let lost = before.saturating_sub(after) as f32;
        Ok(gain - (lost + used) * self.config.safety_bridge_stock_cost)
    }

'''
if anchor not in s: raise SystemExit('decide_ramen anchor missing')
s=s.replace(anchor,method+anchor)
s=s.replace(
'''        let risk = (g.ramen.feeling_stock.iter().sum::<i32>() - self.config.feeling_overflow_threshold).max(0) as f32;
''',
'''        let risk = (g.ramen.feeling_stock.iter().sum::<i32>() - self.config.feeling_overflow_threshold).max(0) as f32;
        let bridge = self.safety_bridge(g, a)?;
''')
s=s.replace(
'''                let window = self.ramen_window_alignment(g, region_id)?;
                let look = self.ramen_lookahead(g, region_id)?;
                o.score += ck + rmj + great + window + look;
''',
'''                let window = self.ramen_window_alignment(g, region_id)?;
                let safety = if let Some((_, gain)) = bridge {
                    self.safety_bridge_candidate(g, region_id, gain)?
                } else { 0.0 };
                let look = self.ramen_lookahead(g, region_id)?;
                o.score += ck + rmj + great + window + safety + look;
''')
s=s.replace(
'''                o.add("ramen_window", window);
                o.add("ramen_lookahead", look)
''',
'''                o.add("ramen_window", window);
                o.add("safety_bridge", safety);
                o.add("ramen_lookahead", look)
''')
p.write_text(s)
