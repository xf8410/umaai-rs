from pathlib import Path

path=Path('crates/umasim/src/trainer/local_ramen_trainer.rs')
s=path.read_text()
if 'recipe_continuity_weight' in s:
    print('配方续航专项入口已存在'); raise SystemExit(0)

repls=[
('''    /// SpecialSelect 是否按吃后库存、后续可制作集合和年末剩余价值动态选择。
    pub dynamic_special_targets: bool''','''    /// SpecialSelect 是否按吃后库存、后续可制作集合和年末剩余价值动态选择。
    pub dynamic_special_targets: bool,

    /// 吃完候选面后，每减少一种仍可制作的已选地区面所收取的续航成本。
    /// 只比较当前实际选中的三种地区面；0 表示关闭。
    pub recipe_continuity_weight: f32'''),
('''            deadline_urgency_scale: 0.0,
            dynamic_special_targets: false''','''            deadline_urgency_scale: 0.0,
            dynamic_special_targets: false,
            recipe_continuity_weight: 0.0'''),
('''    fn safety_bridge_candidate(&self, g: &RamenGame, region_id: usize, gain: f32) -> Result<f32> {''','''    fn recipe_continuity_cost(&self, g: &RamenGame, region_id: usize) -> Result<f32> {
        if self.config.recipe_continuity_weight <= 0.0 { return Ok(0.0); }
        let targets = list_special_targets_for(&g.ramen, region_id)?
            .into_iter().min_by_key(|t| t.iter().sum::<i32>())
            .ok_or_else(|| anyhow::anyhow!("拉面 {region_id} 无合法 targets"))?;
        let craftable = |state: &_| -> usize {
            g.ramen.selected_regions.iter().filter(|&&rid|
                list_special_targets_for(state, rid).map(|x| !x.is_empty()).unwrap_or(false)
            ).count()
        };
        let before = craftable(&g.ramen);
        let mut post = g.ramen.clone();
        consume_for_ramen(&mut post, region_id, &targets)?;
        let after = craftable(&post);
        Ok(before.saturating_sub(after) as f32 * self.config.recipe_continuity_weight)
    }

    fn safety_bridge_candidate(&self, g: &RamenGame, region_id: usize, gain: f32) -> Result<f32> {'''),
('''                let cook2_cost = self.cook2_ramen_stock_cost(g, region_id)?;''','''                let cook2_cost = self.cook2_ramen_stock_cost(g, region_id)?;
                let continuity_cost = self.recipe_continuity_cost(g, region_id)?;'''),
('''                o.score += ck + rmj + great + deadline + window + safety + look - cook2_cost;''','''                o.score += ck + rmj + great + deadline + window + safety + look - cook2_cost - continuity_cost;'''),
('''                o.add("cook2_stock_cost", -cook2_cost);''','''                o.add("cook2_stock_cost", -cook2_cost);
                o.add("recipe_continuity_cost", -continuity_cost);'''),
('''    /// 构造当前正式推荐 preset。
    pub fn new() -> Self {''','''    /// 构造配方续航专项候选；固定前三步共同基线，只覆盖续航成本。
    pub fn with_recipe_continuity_override(weight: f32) -> Self {
        let mut trainer = Self::with_vital_transition_overrides(0, 0, 0.0, 0, true);
        for year in &mut trainer.years { year.config.recipe_continuity_weight = weight; }
        trainer
    }

    /// 构造当前正式推荐 preset。
    pub fn new() -> Self {''')]
for old,new in repls:
    if s.count(old)!=1: raise SystemExit(f'匹配数量错误 {s.count(old)}: {old[:60]}')
    s=s.replace(old,new)
path.write_text(s)
