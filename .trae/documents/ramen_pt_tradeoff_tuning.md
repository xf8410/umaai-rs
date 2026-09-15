# 拉面评分换 PT 调优：手写策略 + MCTS 双路径定案

> 2026-09-14 系列实验与实现记录。
> **当前状态**：手写策略最优档已固化进 preset，MCTS 评分换 PT 走 `pt_favor_rate` 旋钮。
> 本文档记录机制发现、参数设计、标定数据、最终决策与复现方式。

## 1. 背景：三类修复全部降分 → 根因不是"惩罚过严"

曾对 `reserve_penalty` 试过三种修复（`rgn1` 增量截断 / `rgn2` 满位豁免 / `reserve` 调低
0/20），100 局基准**均未提升总评分**（-32 或持平）。结论：

- `reserve_penalty` 的双重计罚虽"数值虚增"，但其**方向性惩罚恰好挡住**"终盘练已满位
  拿 PT"的亏分陷阱（策略内 1pt=64 分，终局实际 2 分/pt，高估约 32 倍）；
- 直接修惩罚 = 放开陷阱 = 降分。用户"reserve_penalty 不可删除"的判断得到印证。

真正的偏差在 **PT 估值本身**，而非惩罚。

## 2. 前提验证：超级拉面（turn 72-77）与 PT 产出分布

**超级拉面三次训练是全局峰值**（`calc_finals_effect`：youqing=150 / pt_bonus=100 →
PT 上限 100→350）。实证（90 个候选，seed 61444 前 3 局）：

| 彩圈数 | n | PT 均 | 有效主增 | 副增 |
|---|---:|---:|---:|---:|
| 0 | 18 | 40 | 32 | 11 |
| 1 | 48 | 267 | 151 | 99 |
| 2 | 22 | 284 | 79 | 135 |
| 3 | 2 | 340 | 0 | 204 |

| 分类 | n | PT 均 | 有效主增 |
|---|---:|---:|---:|
| 已满位 | 28 | 232 | 0 |
| 未满位 | 62 | 225 | 154 |

**关键事实**：
1. PT 产出由**彩圈数（友情训练）**主导，与"属性是否已满"几乎无关（已满 232 vs 未满
   225）——"满位只剩 PT"是错的，**满位的 PT 并不比未满位少**；
2. 彩圈同时放大属性和 PT（同一 youqing 乘数），所以"彩圈多 + 未满"无条件优先；
3. 真正的最差选择是"**0 彩圈 + 已满位**"（PT≈40 且属性 0）。

## 3. 机制：策略对 PT 的估值系统性高估

- 策略内部：训练 PT 按 `pt_rate`（Y1=16 / Y2/Y3=64）折算；比赛/事件同源。
- 终局评分：`skill_pt × pt_score_rate(2.0)`。
- 高估约 32 倍 → 终盘策略把"练满位拿 300 PT"当成 19200 策略分（实际值 600 分），
  为此牺牲未满位训练（属性 +206 + PT 296 双丰收）。

`pt_rate` 影响三处（训练未满位 / 比赛 / 事件），只改它是**负收益**（实测 -145）——
因为未满位训练的 PT 追求路径本身是好的，不该打折。

## 4. 手写策略：已满位 PT 定价（pt_tradeoff）

### 4.1 机制

`score_train_action_eval` 中，`main_full = inc_main>0 && cap_left==0` 时
PT 按 `eff_pt_rate` 折算（而非 `pt_rate`），避免终盘贪练已满位。

根据彩圈数分级定价：
- 无彩圈已满位（PT≈40 且属性 0）：最差选择，用低价**重压**
- 有彩圈已满位（PT 267-340）：唯一值得权衡的纯 PT 来源

### 4.2 标定（seed 61444，7 build × 100 局）

| 有彩圈定价 | 评分 | Δ分 | skill_pt | Δpt | 评价 |
|---:|---:|---:|---:|---:|
| 16 | 64785 | +52 | 8449 | -241 | 过度回避满位 |
| 24 | 65089 | +356 | 8536 | -154 | 偏评分 |
| **36** | **65266** | **+533** | 8600 | -90 | **评分峰值** |
| 44 | 65214 | +481 | 8625 | -65 | 均衡 |
| 48 | 65215 | +483 | 8636 | -54 | 均衡 |
| 52 | 65210 | +478 | 8650 | -40 | 偏 PT |
| 56 | 65127 | +394 | 8663 | -27 | 偏 PT |
| 64（旧行为） | 64733 | 0 | 8690 | 0 | PT 峰值 |
| 80 | 63254 | -1478 | 8688 | -2 | **双降（被支配）** |
| 96 | 62440 | -2293 | 8665 | -25 | 双降 |

### 4.3 最优档固化

| 参数 | 值 | 说明 |
|---|---|---|
| `pt_tradeoff_shining` | 36 | 有彩圈已满位 PT 定价，评分峰值 |
| `pt_tradeoff` | 16 | 无彩圈已满位，恒重压 |

已硬编码进 `RecommendedRamenTrainer::new()` preset，不再暴露为可配参数。
`ramen_pt_sacrifice_score`（玩家 knob）及 `trd`/`trdsh`/`trds`/`ptrate` token 已移除。

## 5. MCTS：评分换 PT 走 `pt_favor_rate`

### 5.1 动机

手写策略 pt_tradeoff 只影响 ~5% 训练决策（已满位），PT 上限 ≈8690 由结构决定。
要大幅换 PT 须走 MCTS 终局估值。

### 5.2 公式

`RamenGame::search_score()` 覆盖 trait 默认：

```
score    = calc_score()                                                    ← 正常评分（始终不变）
score_pt = skill_score + skill_pt × pt_score_rate(2.0) × pt_favor_rate + five_status_scores
```

- `pt_favor_rate = 1.0` 时 `score_pt == calc_score()`，零偏好
- 不乘 ×0.37 缩放——MCTS 只比相对大小，线性变换不改变排序
- `pt_favor_rate` 是唯一旋钮

### 5.3 配置

```toml
# game_config.toml → [config_override]
pt_favor_rate = 1.0   # 默认中性（= calc_score）；>1.0 = 倾向换 PT
```

`pt_favor_rate` 代码默认与 `default_config.toml` 均已 1.0（历史 onsen 用 8.0，已修正）。

### 5.4 架构

- `RamenGame::search_score()` 覆盖 trait 默认（OnsenGame 仍用旧 `calc_score_with_pt_favor`）
- MCTS 动作选择统一走 `best_action_pt_idx()`（读 `score_pt`）
- `RamenSelection`（Score/Pt）枚举从 ramen 侧完全移除，`bench_base --search-selection` 已删
- Rollout 策略仍为 `RecommendedRamenTrainer`（手写策略最优 preset）

**注意**：`pt_favor_rate = 8.0` 会让 MCTS 在 PT×16 目标下狂出行（搜索发现"出行→吃面→训练加成"的 PT 链在终局估值里碾压直接训练）。
故 `default_config.toml` 已从 8.0 改为 1.0，onsen 如需 8.0 需在 onsen 侧显式覆盖。

## 6. 扫参标定（待完成）

MCTS `pt_favor_rate` 扫参：1.0 / 2.0 / 4.0 / 8.0 / 12.0 / 16.0。
命令：

```bash
# 在 game_config.toml 的 [config_override] 中设 pt_favor_rate = X，然后：
./target/release/bench_base --trainer mcts --search-n 1024 --runs 100 --seed 61444
```

## 7. 后续方向

1. **MCTS pt_favor_rate 标定**：产出「设 X → 评分 −Y，技能点 +Z」映射表
2. **彩圈分级的进一步细分**：按彩圈 0/1/2/3 独立分档（当前 0 vs ≥1 两档）
3. **超拉面期间训练位分配**：`pt_tradeoff_super` 独立档已实现但未深度扫描；确认属性 vs PT 优先的更优组合
4. **总 PT 结构杠杆**：吃面节奏 / 超拉面训练位选择 / 事件选择对 PT 上限的影响