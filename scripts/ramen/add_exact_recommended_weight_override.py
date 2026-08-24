from pathlib import Path
import subprocess

path = Path("crates/umasim/src/trainer/local_ramen_trainer.rs")
text = path.read_text(encoding="utf-8")

# CI 诊断/修复任务可能基于已经注入过实验入口的提交继续运行。
# 此时完整补丁（含动态属性字段）已经存在，重复调用应当安全地直接成功，
# 而不是让下游严格单次替换脚本因匹配数为 0 失败。
if "pub fn with_experiment_overrides(" in text:
    print("专项参数入口已存在，跳过重复注入")
    raise SystemExit(0)

# 先加入动态缺口/溢出字段及评分逻辑，再给正式 preset 增加精确隔离的实验入口。
subprocess.run(["python3", "scripts/ramen/add_dynamic_status_balance_v39.py"], check=True)

text = path.read_text(encoding="utf-8")
needle = '''impl RecommendedRamenTrainer {
    /// 构造当前正式推荐 preset。
    pub fn new() -> Self {'''
replacement = '''impl RecommendedRamenTrainer {
    /// 从正式 preset 精确复制，只覆盖专项矩阵明确列出的评分参数。
    ///
    /// 吃面事务门、体力硬门、友人 0/2/5 节奏、动态事件、隐藏风味等结构逻辑
    /// 均逐字继承 `new()`，防止实验候选混入未声明的策略差异。
    pub fn with_experiment_overrides(
        pt_rates: [f32; 3],
        gap_strength: f32,
        overflow_strength: f32,
        max_base_score_sacrifice: f32,
        ramen_window_weight: f32,
        status_reserve_max: f32,
        early_bond_value: f32,
        hint_bonus: f32,
    ) -> Self {
        let mut trainer = Self::new();
        for (year, pt_rate) in trainer.years.iter_mut().zip(pt_rates) {
            year.policy.config.pt_rate = pt_rate;
            year.config.dynamic_status_balance = gap_strength != 0.0 || overflow_strength != 0.0;
            year.config.status_gap_strength = gap_strength;
            year.config.status_overflow_strength = overflow_strength;
            year.config.max_base_score_sacrifice = max_base_score_sacrifice;
            year.config.ramen_window_weight = ramen_window_weight;
            year.config.status_reserve_max = status_reserve_max;
            year.config.early_bond_value = early_bond_value;
            year.config.hint_bonus = hint_bonus;
        }
        trainer
    }

    /// 构造当前正式推荐 preset。
    pub fn new() -> Self {'''
if text.count(needle) != 1:
    raise SystemExit(f"expected exactly one RecommendedRamenTrainer impl marker, got {text.count(needle)}")
text = text.replace(needle, replacement)
path.write_text(text, encoding="utf-8")
