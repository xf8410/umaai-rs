from pathlib import Path

path=Path('crates/umasim/src/trainer/local_ramen_trainer.rs')
s=path.read_text()
if 'pub fn with_random_decimal_overrides(' in s:
    print('小数随机权重联合入口已存在'); raise SystemExit(0)
needle='''    /// 构造当前正式推荐 preset。
    pub fn new() -> Self {'''
replacement='''    /// 构造暴力随机小数权重候选。
    ///
    /// 仅供 CI 矩阵联合搜索；结构守门、友人节奏和已确认的第3步结论保持不变。
    pub fn with_random_decimal_overrides(
        pt_rates: [f32; 3],
        gap_strength: f32,
        overflow_strength: f32,
        max_base_score_sacrifice: f32,
        y12_window: f32,
        y3_window: f32,
        status_reserve_max: f32,
        early_bond_value: f32,
        hint_bonus: f32,
        recipe_continuity_weight: f32,
    ) -> Self {
        let mut trainer = Self::with_experiment_overrides(
            pt_rates,
            gap_strength,
            overflow_strength,
            max_base_score_sacrifice,
            y12_window,
            status_reserve_max,
            early_bond_value,
            hint_bonus,
        );
        trainer.years[0].config.ramen_window_weight = y12_window;
        trainer.years[1].config.ramen_window_weight = y12_window;
        trainer.years[2].config.ramen_window_weight = y3_window;
        for year in &mut trainer.years {
            year.config.recipe_continuity_weight = recipe_continuity_weight;
            year.config.y3_pre_train_vital_target = 0;
            year.config.y3_post_train_vital_target = 0;
            year.config.y3_vital_shortfall_weight = 0.0;
            year.config.y3_post_train_hard_floor = 0;
        }
        trainer
    }

    /// 构造当前正式推荐 preset。
    pub fn new() -> Self {'''
if s.count(needle)!=1: raise SystemExit(f'构造器标记匹配数量错误: {s.count(needle)}')
path.write_text(s.replace(needle,replacement))
