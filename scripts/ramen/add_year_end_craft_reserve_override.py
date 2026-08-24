from pathlib import Path

path=Path('crates/umasim/src/trainer/local_ramen_trainer.rs')
s=path.read_text()
if 'year_end_craft_reserve_penalty' in s:
    print('年末保线专项入口已存在'); raise SystemExit(0)

repls=[
('''    /// 吃完候选面后，每减少一种仍可制作的已选地区面所收取的续航成本。
    /// 只比较当前实际选中的三种地区面；0 表示关闭。
    pub recipe_continuity_weight: f32''','''    /// 吃完候选面后，每减少一种仍可制作的已选地区面所收取的续航成本。
    /// 只比较当前实际选中的三种地区面；0 表示关闭。
    pub recipe_continuity_weight: f32,

    /// 距离本年 RMJ 结算不超过多少回合时启用“至少保留一碗可做面”。0 表示关闭。
    pub year_end_craft_reserve_window: i32,

    /// 未达本年普通成功线、且当前吃面后不再有任何已选地区面可制作时的惩罚。
    pub year_end_craft_reserve_penalty: f32'''),
('''            dynamic_special_targets: false,
            recipe_continuity_weight: 0.0''','''            dynamic_special_targets: false,
            recipe_continuity_weight: 0.0,
            year_end_craft_reserve_window: 0,
            year_end_craft_reserve_penalty: 0.0'''),
('''    fn safety_bridge_candidate(&self, g: &RamenGame, region_id: usize, gain: f32) -> Result<f32> {''','''    fn year_end_craft_reserve_cost(&self, g: &RamenGame, region_id: usize, post_pt: i32) -> Result<f32> {
        let window = self.config.year_end_craft_reserve_window;
        let penalty = self.config.year_end_craft_reserve_penalty;
        if window <= 0 || penalty <= 0.0 { return Ok(0.0); }
        let turns_left = (Self::year_end(g) - g.turn()).max(0);
        if turns_left > window { return Ok(0.0); }
        let year = (g.current_year() - 1).clamp(0, 2) as usize;
        let data = RAMENDATA.get().ok_or_else(|| anyhow::anyhow!("RAMENDATA 未初始化"))?;
        let target = *data.ramen_success_pt.get(year).unwrap_or(&i32::MAX);
        // 当前碗已经确保过普通成功线时无需再保线。
        if post_pt >= target { return Ok(0.0); }
        let targets = list_special_targets_for(&g.ramen, region_id)?
            .into_iter().min_by_key(|t| t.iter().sum::<i32>())
            .ok_or_else(|| anyhow::anyhow!("拉面 {region_id} 无合法 targets"))?;
        let mut post = g.ramen.clone();
        consume_for_ramen(&mut post, region_id, &targets)?;
        let after = post.selected_regions.iter().filter(|&&rid|
            list_special_targets_for(&post, rid).map(|x| !x.is_empty()).unwrap_or(false)
        ).count();
        if after > 0 { return Ok(0.0); }
        let urgency = (window - turns_left + 1) as f32 / window.max(1) as f32;
        Ok(penalty * urgency)
    }

    fn safety_bridge_candidate(&self, g: &RamenGame, region_id: usize, gain: f32) -> Result<f32> {'''),
('''                let continuity_cost = self.recipe_continuity_cost(g, region_id)?;''','''                let continuity_cost = self.recipe_continuity_cost(g, region_id)?;
                let year_end_reserve_cost = self.year_end_craft_reserve_cost(g, region_id, post)?;'''),
('''                o.score += ck + rmj + great + deadline + window + safety + look - cook2_cost - continuity_cost;''','''                o.score += ck + rmj + great + deadline + window + safety + look - cook2_cost - continuity_cost - year_end_reserve_cost;'''),
('''                o.add("recipe_continuity_cost", -continuity_cost);''','''                o.add("recipe_continuity_cost", -continuity_cost);
                o.add("year_end_craft_reserve", -year_end_reserve_cost);'''),
('''    /// 构造当前正式推荐 preset。
    pub fn new() -> Self {''','''    /// P0第5步年末保线候选；以第三代胜出锚点H0为共同基线。
    pub fn with_year_end_craft_reserve_override(window: i32, penalty: f32) -> Self {
        let mut trainer = Self::with_random_decimal_overrides(
            [31.0594, 26.2536, 4.3471], 1.1311, 1.3254, 234.9200,
            0.1148, 0.1999, 5.2663, 5.2657, 11.2926, 0.0,
        );
        for year in &mut trainer.years {
            year.config.year_end_craft_reserve_window = window;
            year.config.year_end_craft_reserve_penalty = penalty;
        }
        trainer
    }

    /// 构造当前正式推荐 preset。
    pub fn new() -> Self {''')]
for old,new in repls:
    if s.count(old)!=1: raise SystemExit(f'匹配数量错误 {s.count(old)}: {old[:80]}')
    s=s.replace(old,new)
path.write_text(s)
