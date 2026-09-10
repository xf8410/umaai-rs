<div align="center">

# 🦀 umaai-rs（上游 fork）

**UmaAi 的 Rust 重写：育成 AI 主仓 + 拉面杯策略实验与 PR 协作**

![分支](https://img.shields.io/badge/分支-37-10B981?style=flat-square) ![版本](https://img.shields.io/badge/版本-0-F59E0B?style=flat-square) ![CI](https://img.shields.io/badge/CI-29-3B82F6?style=flat-square)

</div>

---
> 📌 **一句话定位**：Rust 重写 UmaAi 的育成 AI 主仓（上游 xulai1001/umaai-rs），本 fork 承载拉面杯策略实验与 PR 往返。

## 🧭 项目定位

<b>umaai-rs</b> 是 UmaAi（C++ 育成 AI）的 Rust 重写主仓，上游维护者为 xulai1001，含 OnsenGame 完整实现。本 fork 用于<b>拉面杯（剧本14）策略的实验迭代与上游 PR 协作</b>：策略收敛、配卡矩阵、修复与回灌上游都走这里的 workbench 分支。

## ✨ 核心功能
- Rust 重写育成 AI（MCTS 决策）
- 拉面杯策略：前瞻 v9、动态朋友外出、吃后训练事务、有效失败感知窗口
- 配卡矩阵：101 配卡 × 120 拉面地区排列组合、全配卡穷举、技能点权重蒸馏
- 温泉（Onsen）剧本完整实现与备份
- 上游 PR 往返：策略文件导出/格式修复/发布检查

## 🌿 分支导览（共 37 个分支分组说明）

<details open>
<summary><b>点击收起/展开全部分支用途说明</b></summary>

| 分支 | 用途说明 |
|---|---|
| `master` | 上游主干（跟随 xulai1001/umaai-rs） |
| `upstream` | 上游同步跟踪线 |
| `baseline/upstream-ramen_workbench` | 上游拉面工作台基线 |
| `onsen_backup` | 温泉剧本备份线 |
| `workbench/ramen-strategy-consolidated / -final / -pr / -pr-clean / -pr-v2` | 拉面策略收敛与 PR 提交线（五代） |
| `workbench/fix-ramen-gauge-count(-v2) / fix-ramen-source-lock / fix-ramen-player-main-upstream-pr` | 拉面计数/源锁定/玩家主线上游 PR 修复线 |
| `workbench/101x120-upstream-ramen(-v2) / y3-region-matrix(-corrected) / y3-composition-region-matrix / actual-card-combinations / bench-card-compositions / skill-pt-matrix-distill` | 配卡×地区矩阵实验线 |
| `workbench/ramen-improve-1 / ramen-mcts-iteration / ramen-mine-plan / port-local-ramen-strategy` | 策略迭代/自研计划/本地策略移植线 |
| `workbench/ramen-aligned-20260826 / sync-upstream-ramen-20260826 / resolve-upstream-ramen-20260826 / backup-pre-upstream-sync` | 2026-08-26 上游同步三连 + 同步前备份 |
| `workbench/flat-search-warning-cleanup / format-compositions / ramen-strategy-clean / ramen-workbench-pr-test / fix-y3-composition-region-matrix / upstream-pr-ramen-player-features / validate-flat-search-cleanup` | 清理/格式化/PR 测试等辅助线 |

</details>

## 🏷️ 版本历史

无 release（策略实验仓，产出经 PR 回灌上游）。

完整版本列表 ➡️ [Releases 页](../../releases)

## ⚙️ CI 流水线（共 29 条，拉面策略实验矩阵）

| 流水线 | 用途说明 |
|---|---|
| 每种配卡×120拉面地区排列组合 / 101x120-upstream-ramen | 配卡×地区全组合模拟评估 |
| 全配卡缺口、溢出与技能点权重穷举矩阵 / skill-pt 矩阵 | 全配卡穷举与技能点权重蒸馏 |
| 正式拉面策略精确隔离权重复赛 / 将3速1耐1智设为复赛必测配卡 | 复赛基准与必测配卡 |
| Ramen effective failure-aware lookahead v11 / generic ramen lookahead v9 / v8 window ablation v13 / G39配方续航严格消融 | 前瞻与消融实验系列 |
| Ramen dynamic friend outing v28 / friend outing cross-year pacing v25 | 朋友外出动态化/跨年节奏 |
| Ramen eat-then-train transaction v20 | 吃后训练事务化实验 |
| Adapt Cook2 farm logic to ramen v15 / Minimal strategy A/B / composition profile matrix v30 / Promote recommended v17 | 策略移植/A-B/画像/推荐晋升 |
| Apply ramen handwritten constraint / timing v8 / v7 float literal fix / 拉面特征 schema 矩阵 v31 | 策略补丁应用系列 |
| 捕获结构诊断完整编译错误 / Capture v7 compiler diagnostics / 修复结构诊断CSV格式参数 / 修复拉面杯策略文件格式 / Export ramen strategy TXT | 编译诊断与产物导出 |
| Validate actual card combinations / Format composition benchmark / Document local ramen policy configuration / 目录结构与发布检查 / Apply ramen source lock 修复 | 校验与发布检查 |
