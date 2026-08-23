from pathlib import Path

path = Path("crates/umasim/src/trainer/local_ramen_trainer.rs")
text = path.read_text(encoding="utf-8")
if "pub fn with_vital_transition_overrides(" in text:
    print("吃后体力转移专项入口已存在")
    raise SystemExit(0)

needle = '''    /// 构造当前正式推荐 preset。
    pub fn new() -> Self {'''
replacement = '''    /// 构造吃后体力转移专项候选。
    ///
    /// 固定前两年 PT32/对盘15%、第三年 PT16/对盘12%，只覆盖第三年的
    /// 训练前/后体力预算、短缺成本、硬底线与确定恢复视野。
    pub fn with_vital_transition_overrides(
        pre_target: i32,
        post_target: i32,
        shortfall_weight: f32,
        hard_floor: i32,
        recovery_horizon: bool,
    ) -> Self {
        let mut trainer = Self::with_experiment_overrides(
            [32.0, 32.0, 16.0], 0.75, 1.0, 220.0, 0.15, 20.0, 12.0, 8.0,
        );
        trainer.years[0].config.ramen_window_weight = 0.15;
        trainer.years[1].config.ramen_window_weight = 0.15;
        trainer.years[2].config.ramen_window_weight = 0.12;
        trainer.years[2].config.y3_pre_train_vital_target = pre_target;
        trainer.years[2].config.y3_post_train_vital_target = post_target;
        trainer.years[2].config.y3_vital_shortfall_weight = shortfall_weight;
        trainer.years[2].config.y3_post_train_hard_floor = hard_floor;
        trainer.years[2].config.y3_recovery_horizon = recovery_horizon;
        trainer
    }

    /// 构造当前正式推荐 preset。
    pub fn new() -> Self {'''
if text.count(needle) != 1:
    raise SystemExit(f"构造器标记匹配数量错误: {text.count(needle)}")
path.write_text(text.replace(needle, replacement), encoding="utf-8")
