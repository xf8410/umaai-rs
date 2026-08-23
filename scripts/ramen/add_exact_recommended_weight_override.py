from pathlib import Path
import subprocess

# 先加入动态缺口/溢出字段及评分逻辑，再给正式 preset 增加只覆盖三个待测维度的入口。
subprocess.run(["python3", "scripts/ramen/add_dynamic_status_balance_v39.py"], check=True)

path = Path("crates/umasim/src/trainer/local_ramen_trainer.rs")
text = path.read_text(encoding="utf-8")
needle = '''impl RecommendedRamenTrainer {
    /// 构造当前正式推荐 preset。
    pub fn new() -> Self {'''
replacement = '''impl RecommendedRamenTrainer {
    /// 从正式 preset 精确复制，只覆盖本轮复赛允许变化的 PT、缺口和溢出三个维度。
    /// 其余字段逐字继承 `new()`，用于避免矩阵候选混入结构策略差异。
    pub fn with_weight_overrides(pt_rates: [f32; 3], gap_strength: f32, overflow_strength: f32) -> Self {
        let mut trainer = Self::new();
        for (year, pt_rate) in trainer.years.iter_mut().zip(pt_rates) {
            year.policy.config.pt_rate = pt_rate;
            year.config.dynamic_status_balance = gap_strength != 0.0 || overflow_strength != 0.0;
            year.config.status_gap_strength = gap_strength;
            year.config.status_overflow_strength = overflow_strength;
        }
        trainer
    }

    /// 构造当前正式推荐 preset。
    pub fn new() -> Self {'''
if text.count(needle) != 1:
    raise SystemExit(f"expected exactly one RecommendedRamenTrainer impl marker, got {text.count(needle)}")
text = text.replace(needle, replacement)
path.write_text(text, encoding="utf-8")
