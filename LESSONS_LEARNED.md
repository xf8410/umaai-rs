# 经验总结（diag 静默 + 决策理由，2026-08-29）

## 技术线

1. **运行时开关与编译期裁剪是互补而非替代**：`diag!` 的 `cfg(feature)` 管构建形态（no-diag 零开销），全局 `AtomicBool` + RAII guard 管运行时段落（MCTS rollout 静默）。双层 `#[cfg] if enabled() { ... }` 让两者共存——同一处 explain 块既能在 no-diag 构建下整段消失，又能在 diag 构建下按搜索边界跳过。挂点选在 `search_with_terminal`（rollout 边界）而非各 trainer，使 handwritten/random 自动豁免、新 trainer 自动继承。
2. **数据与文字分离走接口**：决策理由的原始数据（Serialize）经 `DecisionReasonSink` trait 发出，可读文字由渲染函数数据驱动生成——两个出口面向两类消费者（下游程序 / 屏幕），互不污染。核心层只定义 trait，宿主程序（umasim）提供默认实现接日志，将来下游接文件/协议不需要改核心层。
3. **门限先行是日志类功能的性能纪律**：险胜判定（一次分差比较）不通过就直接返回，终局维度分析、JSON 序列化、渲染全部不执行——按需懒计算让"每回合都有搜索"的 MCTS 场景下该功能接近零均摊成本。

## 工作流程线

1. **"分析先行 + 决策点清单"模式再次生效**：先摸清 rollout 输出源（diag!/explain 块/debug!/warn! 各自归属）与配置链（MctsConfig→OverrideMctsConfig→merge→SearchConfig）再动手，方案讨论轮用户只需回答拍板点（门限语义、双出口、风格配置位置），实现一轮通过。
2. **基线对照验证改动无回归**：编译警告与测试失败都用 `git stash` 前后对比确认"存量 vs 新引入"（本次 18 个 no-diag 警告、3 个 baseline 失败均为存量），避免误伤式"顺手修复"扩大改动面。
3. **全局状态测试需显式串行化**：操作进程级开关的测试共用一把 `Mutex` 串行锁，否则 cargo test 并行执行下互相污染断言前提；依赖 `GAMECONSTANTS` 的测试统一走 `get_workspace_root + set_current_dir + init_global` 惯例。
