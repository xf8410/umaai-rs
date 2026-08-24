from pathlib import Path
p=Path('crates/umasim/src/trainer/local_ramen_trainer.rs')
s=p.read_text()
s=s.replace(
'''            rules::{calc_ramen_pt_gain, calc_region_bonus, consume_for_ramen, list_special_targets_for},''',
'''            rules::{calc_ramen_pt_gain, calc_region_bonus, consume_for_ramen, get_recipe, list_special_targets_for},''')
s=s.replace(
'''    pub safety_bridge_stock_cost: f32,
''',
'''    pub safety_bridge_stock_cost: f32,
    /// Cook2-style marginal resource price, adapted to annual ramen-stock resets.
    pub cook2_stock_weight: f32,
''')
s=s.replace(
'''            safety_bridge_stock_cost: 0.0,
''',
'''            safety_bridge_stock_cost: 0.0,
            cook2_stock_weight: 0.0,
''')
s=s.replace(
'''            } else if token == "failmodel" {
''',
'''            } else if let Some(v) = token.strip_prefix("cook2") {
                local.cook2_stock_weight = v.parse()?
            } else if token == "failmodel" {
''')
anchor='''    fn safety_bridge_candidate(&self, g: &RamenGame, region_id: usize, gain: f32) -> Result<f32> {
'''
method='''    /// Adaptation of Cook2::materialEvaluation. A unit from a scarce stock is worth more
    /// than one from a rich stock (concave sqrt utility). Unlike the farm scenario, ramen stock
    /// resets yearly, so its shadow price decays toward the RMJ boundary. Before reaching the
    /// annual success target we discount the price: spending to secure scenario progression is
    /// deliberately preferred, matching Cook2 Y1's aggressive cooking-until-target rule.
    fn cook2_ramen_stock_cost(&self, g: &RamenGame, region_id: usize) -> Result<f32> {
        if self.config.cook2_stock_weight <= 0.0 { return Ok(0.0); }
        let targets = list_special_targets_for(&g.ramen, region_id)?
            .into_iter().min_by_key(|t| t.iter().sum::<i32>())
            .ok_or_else(|| anyhow::anyhow!("拉面 {region_id} 无合法 targets"))?;
        let recipe = get_recipe(region_id)?;
        let net = [recipe[0]-targets[0], recipe[1]-targets[1], recipe[2]-targets[2]];
        let year_end = Self::year_end(g);
        let remaining_fraction = ((year_end - g.turn()).max(0) as f32 / 21.0).clamp(0.0, 1.0);
        let year = (g.current_year()-1) as usize;
        let d = RAMENDATA.get().ok_or_else(|| anyhow::anyhow!("RAMENDATA 未初始化"))?;
        let target = *d.ramen_success_pt.get(year).unwrap_or(&i32::MAX);
        let progression_discount = if g.ramen.scenario_pt < target { 0.35 } else { 1.0 };
        let mut marginal = 0.0;
        for i in 0..3 {
            let before = g.ramen.feeling_stock[i] as f32;
            let after = (g.ramen.feeling_stock[i] - net[i]).max(0) as f32;
            // Bias keeps the derivative finite, as in Cook2's sqrt(count + bias).
            marginal += (before + 2.0).sqrt() - (after + 2.0).sqrt();
        }
        // Hidden flavor is globally flexible, so charge it as two ordinary marginal units.
        let hidden = targets.iter().sum::<i32>() as f32;
        marginal += hidden * 0.50;
        Ok(marginal * self.config.cook2_stock_weight * remaining_fraction * progression_discount)
    }

'''
if anchor not in s: raise SystemExit('anchor missing')
s=s.replace(anchor,method+anchor)
old='''                let window = self.ramen_window_alignment(g, region_id)?;
                let safety = if let Some((_, gain)) = bridge {
'''
new='''                let window = self.ramen_window_alignment(g, region_id)?;
                let cook2_cost = self.cook2_ramen_stock_cost(g, region_id)?;
                let safety = if let Some((_, gain)) = bridge {
'''
if old not in s: raise SystemExit('score anchor missing')
s=s.replace(old,new)
s=s.replace(
'''                o.score += ck + rmj + great + window + safety + look;
''',
'''                o.score += ck + rmj + great + window + safety + look - cook2_cost;
''')
s=s.replace(
'''                o.add("ramen_window", window);
                o.add("safety_bridge", safety);
''',
'''                o.add("ramen_window", window);
                o.add("cook2_stock_cost", -cook2_cost);
                o.add("safety_bridge", safety);
''')
p.write_text(s)
