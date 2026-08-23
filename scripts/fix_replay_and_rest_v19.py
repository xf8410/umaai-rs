from pathlib import Path

# 1) Repair detailed replay mode using stable anchors from the formatted source.
p = Path("crates/umasim/src/bin/ramen_low_score_diagnostic.rs")
s = p.read_text()
anchor = '''    init_global_with_config(&load_game_config()?)?;

    let runs: u64 = env::var("DIAG_RUNS").unwrap_or_else(|_| "150".into()).parse()?;
'''
insert = '''    init_global_with_config(&load_game_config()?)?;

    // 详细重放模式：只重跑指定局。启用 diag feature 时，规则层会把每回合状态、
    // 训练面板、库存、拉面效果和事件直接打印到 CI 的可展开步骤中。
    if let Ok(text) = env::var("DIAG_REPLAY_IDX") {
        let run_idx: u64 = text.parse()?;
        println!("================ 低分局详细重放开始 ================");
        println!("BASE_SEED={BASE_SEED}, run_idx={run_idx}");
        let (outcome, trainer) = run(run_idx, true)?;
        let log = trainer.take_records();
        println!("================ 策略逐决策摘要 ================");
        for row in &log.rows {
            println!(
                "[回合 {:02}][{}] 候选={} 选择 #{}: {}",
                row.turn, row.stage, row.candidates, row.action_index, row.action_desc
            );
            if let Some(breakdown) = &row.score_breakdown {
                println!("  候选评分: {breakdown}");
            }
        }
        println!("================ 详细重放终局 ================");
        println!(
            "最终评分={} 等级={} 五维={:?} 五维和={} 技能PT={} 剧本PT={} RMJ={}/3 吃面={} 友人完成={}",
            outcome.score,
            outcome.rank,
            outcome.five_status,
            outcome.five_status.iter().sum::<i32>(),
            outcome.skill_pt,
            outcome.scenario_pt,
            outcome.rmj_ok,
            outcome.eat_count,
            outcome.friend_all
        );
        return Ok(());
    }

    let runs: u64 = env::var("DIAG_RUNS").unwrap_or_else(|_| "150".into()).parse()?;
'''
if s.count(anchor) != 1:
    raise SystemExit(f"replay anchor count={s.count(anchor)}")
s = s.replace(anchor, insert)
p.write_text(s)

# 2) Add a matrix token for controlled hard-rest ablation and relax the production preset.
p = Path("crates/umasim/src/trainer/local_ramen_trainer.rs")
s = p.read_text()
anchor = '''            } else if let Some(v) = token.strip_prefix("cook2") {
                local.cook2_stock_weight = v.parse()?
            } else if token == "failmodel" {
'''
replace = '''            } else if let Some(v) = token.strip_prefix("cook2") {
                local.cook2_stock_weight = v.parse()?
            } else if let Some(v) = token.strip_prefix("vrest") {
                policy.vital_rest = v.parse()?
            } else if token == "failmodel" {
'''
if s.count(anchor) != 1:
    raise SystemExit(f"vrest parser anchor count={s.count(anchor)}")
s = s.replace(anchor, replace)

# The preset has a nested make function; pass the yearly hard threshold explicitly.
s = s.replace('''        fn make(pt_rate: f32) -> LocalRamenTrainer {
''', '''        fn make(pt_rate: f32, vital_rest: i32) -> LocalRamenTrainer {
''', 1)
s = s.replace('''            policy.pt_rate = pt_rate;
            policy.ramen_pt_weight = 2.0;
''', '''            policy.pt_rate = pt_rate;
            policy.ramen_pt_weight = 2.0;
            // 只在极低体力时保留下限；第三年彻底取消硬休息门，交给连续体力、
            // 失败期望与休息动作本身的分数比较，避免浪费终盘高价值训练回合。
            policy.vital_rest = vital_rest;
''', 1)
s = s.replace('''            years: [make(16.0), make(64.0), make(64.0)],
''', '''            years: [make(16.0, 30), make(64.0, 30), make(64.0, 0)],
''', 1)
# Keep preset docs accurate.
s = s.replace('''/// - 关闭随机分身 lookahead。
''', '''/// - 关闭随机分身 lookahead；
/// - 第一/二年仅在体力低于 30 时硬休息，第三年取消硬休息门，改由连续评分决策。
''', 1)
p.write_text(s)
