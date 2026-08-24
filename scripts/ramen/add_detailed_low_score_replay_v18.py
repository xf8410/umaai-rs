from pathlib import Path
p=Path('crates/umasim/src/bin/ramen_low_score_diagnostic.rs')
s=p.read_text()
s=s.replace(
'''    let runs: u64 = env::var("DIAG_RUNS").unwrap_or_else(|_| "150".into()).parse()?;
    let output_dir = PathBuf::from(env::var("DIAG_OUTPUT_DIR").unwrap_or_else(|_| "benchmark-results/low-score-v17".into()));
    fs_err::create_dir_all(&output_dir)?;

    let mut outcomes = Vec::with_capacity(runs as usize);
''',
'''    // 详细重放模式：只跑指定局，不扫描样本窗口。配合 `--features diag` 使用时，
    // 规则层会把每回合状态、训练面板、拉面库存和实际效果直接写入 CI 日志。
    if let Ok(text) = env::var("DIAG_REPLAY_IDX") {
        let run_idx: u64 = text.parse()?;
        println!("================ 低分局详细重放开始 ================");
        println!("BASE_SEED={BASE_SEED}, run_idx={run_idx}");
        let (outcome, trainer) = run(run_idx, true)?;
        let log = trainer.take_records();
        println!("================ 策略逐决策摘要 ================");
        for row in &log.rows {
            println!("[回合 {:02}][{}] 候选={} 选择 #{}: {}", row.turn, row.stage, row.candidates, row.action_index, row.action_desc);
            if let Some(breakdown) = &row.score_breakdown {
                println!("  候选评分: {breakdown}");
            }
        }
        println!("================ 详细重放终局 ================");
        println!("最终评分={} 等级={} 五维={:?} 五维和={} 技能PT={} 剧本PT={} RMJ={}/3 吃面={} 友人完成={}",
            outcome.score, outcome.rank, outcome.five_status, outcome.five_status.iter().sum::<i32>(),
            outcome.skill_pt, outcome.scenario_pt, outcome.rmj_ok, outcome.eat_count, outcome.friend_all);
        return Ok(());
    }

    let runs: u64 = env::var("DIAG_RUNS").unwrap_or_else(|_| "150".into()).parse()?;
    let output_dir = PathBuf::from(env::var("DIAG_OUTPUT_DIR").unwrap_or_else(|_| "benchmark-results/low-score-v17".into()));
    fs_err::create_dir_all(&output_dir)?;

    let mut outcomes = Vec::with_capacity(runs as usize);
''')
s=s.replace(
'''    fs_err::write(output_dir.join("lowest-report.md"), lines.concat())?;
    println!("最低局 run_idx={low_idx}, score={}, 输出目录={}", low.score, output_dir.display());
''',
'''    fs_err::write(output_dir.join("lowest-report.md"), lines.concat())?;
    fs_err::write(output_dir.join("lowest-run-idx.txt"), format!("{low_idx}"))?;
    println!("最低局 run_idx={low_idx}, score={}, 输出目录={}", low.score, output_dir.display());
''')
p.write_text(s)
