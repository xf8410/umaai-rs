# NN rollout 基策 vs 手写 —— 完整育成（stages=all）判负

- 权重：release `nn-ramen-v1`（754→245，训练数据仅 1527 条）
- 配置：search_n=8，seed=61444，CRN 同 seed 配对，run 39193332436

## 两种口径的天壤之别

| 口径 | 手写均值 | NN 均值 | 配对差 | t | 胜率 |
|---|---|---|---|---|---|
| train（只搜训练阶段） | 52809.6 | 57387.3 | +4578 | +7.87 | 88% |
| **all（完整育成）** | 52202.8 | 49899.8 | **−2303** | **−3.26** | **28%** |

## 判读结论（关键）

**权重「没有提升」，判负，需更新权重（重训）。**

- train 阶段 NN 显著好，但完整育成口径 NN 显著差——局部口径的正向是假象
- 这正好印证 DESIGN.md 的铁律：**「最终是否超过手写策略仍只看 Rust 侧纯网络完整育成基准」**
- handoff 文档早警告过：NN 完整续跑未证明普遍强

## 根因诊断（已实锤，含结构性采集盲区）

当前权重训练数据仅 **1527 条**（来自 prod_n4096 的 p0 一个 part），stage 分布严重失衡：

| stage | 决策点/局 | 训练样本 | 占比 |
|---|---|---|---|
| RamenSelect | 61 | 980 | 64% |
| Train | 69 | 455 | 30% |
| RegionSelect | 3 | 88 | 5.8% |
| SuperRamenSelect | 1 | 4 | 0.3% |
| **SpecialSelect** | **25** | **0** | **0%** |

**反转真相**：train 口径赢（Train 阶段 NN 学得好），all 口径输（完整育成暴露 NN 在 SpecialSelect 弃守 + 弱 stage 崩坏）。

**SpecialSelect 为什么恒 0（结构性，非数据量）**：
- 采样器默认 `capture_stage=None` = 白名单内**首个**合格决策点
- 每回合白名单顺序 `RamenSelect → SpecialSelect → Train`，RamenSelect 候选多几乎必先命中 → SpecialSelect 永远轮不到
- 作者为 RegionSelect 单独做了 `region_quota_permille` 配额（所以 RegionSelect 采到 1473 条），**SpecialSelect 无对应配额 → 结构性 0**
- SpecialSelect 仅在「吃面」时触发（`pending_ramen` 非 None），见 game.rs

**data754 各数据集 SpecialSelect 全是 0**（r2roll 23522 条亦然），证明这是采样器固有分布，非数据量问题。

**重训两阶段**：
- Phase A（低成本）：用 r2roll 23522 条（15×数据）重训，RamenSelect/Train/RegionSelect/SuperRamenSelect 大幅改善；SpecialSelect 仍 0
- Phase B（根本解）：给 sampler 加 SpecialSelect 配额（仿 region_quota_permille），补采 SpecialSelect 数据再并入

## 良性/坏种子（all 口径）

- 良性 14 个（均差 +3635.5）
- 坏 36 个（均差 −4612.4），最差 run27(−12192)、run35(−10075)、run12(−9901)

## 口径备注

nn_rollout_probe 测的是「rollout 基策」；权威验收（叶 value / teacher-game）仍需 GPU 侧车。但 stages=all 负向已是强信号。