//! 在固定 seed 窗口中寻找推荐拉面策略的低分局，并重跑最低局导出决策轨迹。
//!
//! 目的不是只展示“最低分”，而是给出可定位的证据：终局属性、技能 PT、剧本 PT、
//! RMJ 成功数、吃面次数、友人链完成情况，以及每个阶段的选中动作与候选评分。

use std::{collections::BTreeMap, env, path::PathBuf};

use anyhow::Result;
use umasim::{
    bench::{self, GameOutcome},
    game::InheritInfo,
    gamedata::{GAMECONSTANTS, init_global_with_config},
    global,
    trainer::{LoggingTrainer, RecommendedRamenTrainer},
    utils::{get_workspace_root, load_game_config}
};

const BASE_SEED: u64 = 61_444;
const UMA: u32 = 102_601;
const DECK: [u32; 6] = [302424, 302894, 303044, 302924, 303024, 303054];
const INHERIT: InheritInfo = InheritInfo {
    blue_count: [15, 0, 0, 0, 3],
    extra_count: [10, 10, 20, 20, 20, 40]
};

fn run(run_idx: u64, logging: bool) -> Result<(GameOutcome, LoggingTrainer<RecommendedRamenTrainer>)> {
    let mut trainer = LoggingTrainer::new(RecommendedRamenTrainer::new(), run_idx);
    trainer.set_logging(logging);
    let outcome = bench::run_seeded(UMA, &DECK, &INHERIT, BASE_SEED, run_idx, &trainer)?;
    Ok((outcome, trainer))
}

fn main() -> Result<()> {
    std::env::set_current_dir(get_workspace_root()?)?;
    init_global_with_config(&load_game_config()?)?;

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
            "最终评分={} 等级={} 五维={:?} 五维和={} 技能PT={} 剧本PT={:?} RMJ={}/3 吃面={:?} 友人完成={}",
            outcome.score,
            outcome.rank,
            outcome.five_status,
            outcome.five_status.iter().sum::<i32>(),
            outcome.skill_pt,
            outcome.yearly_scenario_pt,
            outcome.rmj_ok,
            outcome.yearly_eat_count,
            outcome.friend_all
        );
        return Ok(());
    }

    let runs: u64 = env::var("DIAG_RUNS").unwrap_or_else(|_| "150".into()).parse()?;
    let output_dir =
        PathBuf::from(env::var("DIAG_OUTPUT_DIR").unwrap_or_else(|_| "benchmark-results/low-score-v17".into()));
    fs_err::create_dir_all(&output_dir)?;

    let mut outcomes = Vec::with_capacity(runs as usize);
    for run_idx in 0..runs {
        let (outcome, _) = run(run_idx, false)?;
        outcomes.push((run_idx, outcome));
    }
    outcomes.sort_by_key(|(_, outcome)| outcome.score);

    let low_idx = outcomes[0].0;
    let (low, trainer) = run(low_idx, true)?;
    let log = trainer.take_records();
    log.save_to(&output_dir.join("lowest-decision-log.csv"))?;

    let mut stage_counts: BTreeMap<String, usize> = BTreeMap::new();
    let mut selected_counts: BTreeMap<String, usize> = BTreeMap::new();
    for row in &log.rows {
        *stage_counts.entry(row.stage.clone()).or_default() += 1;
        *selected_counts
            .entry(format!("{} :: {}", row.stage, row.action_desc))
            .or_default() += 1;
    }

    let scores: Vec<i32> = outcomes.iter().map(|(_, outcome)| outcome.score).collect();
    let mean = scores.iter().map(|&x| x as f64).sum::<f64>() / scores.len() as f64;
    let median = scores[scores.len() / 2];
    let status_sum: i32 = low.five_status.iter().sum();
    let derived_decision_seed = umasim::rng::derive_seed(BASE_SEED, &[low_idx, umasim::rng::DECISION_TAG]);

    let mut lines = Vec::new();
    lines.push("# 推荐拉面策略低分局诊断 v17\n\n".to_string());
    lines.push(format!(
        "> 在 `run_idx=0..{}` 的同一固定窗口中寻找最低分，策略为正式 `RecommendedRamenTrainer`。\n\n",
        runs - 1
    ));
    lines.push("## 分数位置\n\n".to_string());
    lines.push(format!("- 样本数：{runs}\n"));
    lines.push(format!("- 平均最终评分：{mean:.1}\n"));
    lines.push(format!("- 中位最终评分：{median}\n"));
    lines.push(format!("- **最低最终评分：{}**\n", low.score));
    lines.push(format!("- 最低局相对均值：{:+.1}\n", low.score as f64 - mean));
    lines.push(format!("- `run_idx`：`{low_idx}`\n"));
    lines.push(format!("- 基种子：`{BASE_SEED}`\n"));
    lines.push(format!("- 规则主种子：`{:#018x}`\n", low.seed));
    lines.push(format!("- 决策 RNG 种子：`{derived_decision_seed:#018x}`\n\n"));

    lines.push("## 最低局终局状态\n\n".to_string());
    lines.push("|指标|结果|\n|---|---:|\n".to_string());
    lines.push(format!("|最终评分|{}|\n", low.score));
    lines.push(format!("|等级|{}|\n", low.rank));
    lines.push(format!("|技能 PT|{}|\n", low.skill_pt));
    lines.push(format!("|剧本 PT|{:?}|\n", low.yearly_scenario_pt));
    lines.push(format!("|五维|`{:?}`|\n", low.five_status));
    lines.push(format!("|五维原值和|{status_sum}|\n"));
    lines.push(format!("|RMJ 成功年数|{}/3|\n", low.rmj_ok));
    lines.push(format!("|最终当年吃面次数|{:?}|\n", low.yearly_eat_count));
    lines.push(format!(
        "|友人五段全部完成|{}|\n\n",
        if low.friend_all { "是" } else { "否" }
    ));

    lines.push("## 自动风险标记\n\n".to_string());
    let mut flags = Vec::new();
    if low.rmj_ok < 3 {
        flags.push(format!("- ⚠ RMJ 仅成功 {}/3 年，存在明确剧本目标损失。", low.rmj_ok));
    }
    if !low.friend_all {
        flags.push("- ⚠ 友人五段未全部完成，可能损失事件收益或资源。".to_string());
    }
    if low.skill_pt < 6500 {
        flags.push(format!(
            "- ⚠ 技能 PT 仅 {}，明显低于当前策略约 7210 的样本均值。",
            low.skill_pt
        ));
    }
    if status_sum < 9000 {
        flags.push(format!("- ⚠ 五维原值和仅 {status_sum}，训练产出或属性结构可能异常。"));
    }
    if flags.is_empty() {
        flags.push("- 未命中简单终局阈值；应从逐回合窗口、失败和资源时序继续检查。".to_string());
    }
    lines.extend(flags.into_iter().map(|x| format!("{x}\n")));
    lines.push("\n".to_string());

    lines.push("## 最低五局\n\n".to_string());
    lines.push(
        "|排名|run_idx|最终评分|技能PT|五维原值和|剧本PT|RMJ|友人完成|\n|---:|---:|---:|---:|---:|---:|---:|:---:|\n"
            .to_string()
    );
    for (rank, (idx, outcome)) in outcomes.iter().take(5).enumerate() {
        lines.push(format!(
            "|{}|`{}`|{}|{}|{}|{:?}|{}/3|{}|\n",
            rank + 1,
            idx,
            outcome.score,
            outcome.skill_pt,
            outcome.five_status.iter().sum::<i32>(),
            outcome.yearly_scenario_pt,
            outcome.rmj_ok,
            if outcome.friend_all { "是" } else { "否" }
        ));
    }

    lines.push("\n## 决策阶段计数\n\n".to_string());
    lines.push("|阶段|决策次数|\n|---|---:|\n".to_string());
    for (stage, count) in stage_counts {
        lines.push(format!("|`{stage}`|{count}|\n"));
    }

    lines.push("\n## 高频选中动作（前 30）\n\n".to_string());
    lines.push("|次数|阶段与动作|\n|---:|---|\n".to_string());
    let mut selected: Vec<_> = selected_counts.into_iter().collect();
    selected.sort_by_key(|(_, count)| std::cmp::Reverse(*count));
    for (desc, count) in selected.into_iter().take(30) {
        lines.push(format!("|{count}|{}|\n", desc.replace('|', "\\|")));
    }

    lines.push("\n## 人工复核入口\n\n".to_string());
    lines.push("完整逐决策 CSV：`lowest-decision-log.csv`。其中 `score_breakdown` 保存每次决策的全部候选分数与理由，可按 `turn + stage` 回放。\n\n".to_string());
    lines.push(format!(
        "当前评分等级表确认：最低局等级 `{}`；报告生成时全局数据已初始化。\n",
        global!(GAMECONSTANTS).get_rank_name(low.score)
    ));

    fs_err::write(output_dir.join("lowest-report.md"), lines.concat())?;
    println!(
        "最低局 run_idx={low_idx}, score={}, 输出目录={}",
        low.score,
        output_dir.display()
    );
    Ok(())
}
