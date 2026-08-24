# RecommendedTrainer 改进方案 v1

> 起草日期：2026-08-23
> 范围：`RecommendedRamenTrainer`（及其子层 `LocalRamenTrainer` / `RamenPolicy`）+ `LocalRamenTrainer::matrix_variant` DSL

---

## 摘要（修改计划简述）

本次改动围绕三件事：

1. **地区选择要"看出更多东西"**：现状打分只看"卡组构成 × 词条加成"；补三个新指标——地区组合的**总诀窍流入**（`feeling_yield`，哪种组合能产出更多诀窍）、**配方消耗平衡度**（`recipe_balance`，哪种组合配方消耗失衡导致某种诀窍缺/溢）、**卡数少的训练位需要更高友情加成**（`low_count_youqing_bonus`），让不同 build 的卡组能选出不同的地区组合。
2. **第三年体力门禁要"按回合差异化"**：吃面那一回合训练必定成功，可以不卡体力；下一回合没面可吃时训练会失败，必须保留体力兜底。把"全局单一阈值"换成"吃面 / 不吃面两套阈值"，并在 RamenSelect 阶段加"训练后会崩盘"的硬底线。preset 改动按推荐值直接落地。
3. **`matrix_variant` DSL 解析要"数据驱动"**：保留 DSL（shell 网格扫描仍需要 token 串），但用 `lexopt::Parser::from_args` + `TOKEN_SPECS` 常量表 + `RequiredFlags` 结构体重写——新增字段只改表 + 枚举，编译器穷举校验。

---

## 1. 地区选择策略

### 1.1 核心结论

现状打分是"卡组构成 × 词条加成"的纯静态模型——同一卡组不同地区只看词条，**看不到**："哪种地区组合能产出更多总诀窍 / 哪种组合配方消耗失衡导致缺或溢 / 卡数少的训练位需要靠地区友情补"。

把打分升级为**静态词条 + 库存前瞻 + 卡少加权**三层，并新增 build 区分度。

### 1.2 新打分的参考公式

```
score_region_v2 = base_score + feeling_yield + recipe_balance + low_count_youqing_bonus
```

（公式不含 PT 跨档项——`calc_region_bonus` 是全局 PT 阈值，不在 `RegionEffect` 里，地区本身决定不了这个收益。）

各项含义：

- **`base_score`**：现状 sum（卡组 bias × 词条 + PT + Hint），保留不变
- **`feeling_yield`**：**新增**——哪种地区组合能产出更多**总诀窍**。`base_dist = calc_gauge_base_distribution(combo)` 决定了三槽每回合填的基础值；组合总诀窍流入 = Σ base_dist（年度回合数 × 角标触发率近似为常数）。总和越大，每回合清零 +1 诀窍的次数越多，全年累积诀窍越多
- **`recipe_balance`**：**新增**——哪种组合配方消耗结构**失衡**。3 个地区映射到 3 个配方 `region_feeling[rid % len]` 后累加消耗向量，若某类诀窍消耗远高于其他类（缺）或远低于其他类（溢，浪费库存上限 10）则失衡
- **`low_count_youqing_bonus`**：**新增**——卡组中卡数少的训练位（bias ≤ 1）所对应的地区 youqing 项加权放大（弥补"卡少 → 训练加成利用不充分"）

### 1.3 三个指标的思路

#### `feeling_yield`

`base_dist = calc_gauge_base_distribution(combo)` 按三个地区配方的消耗比例分配到三槽——**不同地区组合会得到不同的 base_dist 总和**。比如组合 A 的 base_dist = [4, 3, 3]，组合 B 的 base_dist = [3, 3, 4]，两者总和都是 10；但若 C = [5, 3, 2]，总和 = 10，但其中 A 类更容易凑满 GAUGE_LIMIT=7 清零。

候选思路（具体由实现者定）：

- **简单总和**：`stock_value = Σ base_dist[i]`（每回合总流入越多越值）
- **加权最高槽**：`stock_value = base_dist.max()`（更偏激地看"哪种槽最易凑满"）
- **预期清零次数**：基于年度回合数模拟 `feeling_slot[i]` 累计 → 触发清零次数

矩阵验证哪个涨分再选。

#### `recipe_balance`

3 个地区映射到 3 个配方后，累加消耗向量：

```
recipe_sum[i] = Σ recipe[combo[j] % len][i]   // i = 0..2（A/B/C）
total = Σ recipe_sum[i]                        // = 5 × 3 = 15（每年 3 碗 × 5 消耗）
```

**失衡的两类风险**：

- **缺**：若 `recipe_sum[某类]` 远高于其他 → 该类诀窍消耗快、库存容易清空
- **溢**：若 `recipe_sum[某类]` 远低于其他 → 该类诀窍凑齐后无消耗，溢出上限 10 被浪费

候选思路（具体由实现者定）：

- **标准差**：`imbalance = stddev(recipe_sum)`（越小越均衡；评分用 `-imbalance`）
- **最大/最小比**：`imbalance = max(recipe_sum) / min(recipe_sum).max(1)`（越接近 1 越均衡）
- **与均匀消耗的距离**：每个 `recipe_sum[i] / total` 偏离 `1/3` 的累计值

第1/2 年 `region_id ∈ [0,4]/[5,9]`，取模后配方索引基本唯一，`recipe_sum` 各分量取决于 `region_feeling` 排列；第3年 10 个地区复用 5 个配方，**3 个地区映射到不同配方**的概率显著降低，失衡风险更高——所以主要影响第3年。

#### `low_count_youqing_bonus`

现状 `score_region` 把 youqing 权重只设为 `1.0`（与 xunlian 的 `40` 差 40 倍），导致"卡数少的训练位"完全靠不上地区友情加成——卡组里只有 1 张智卡，那张智卡对应的训练加成再怎么算也撑不起来。

思路：给"卡少的训练位"对应的地区 youqing 加权放大。具体公式待矩阵验证（候选思路）：

- 候选 A：`bias[i] <= 1` 时该位置权重乘 `K`（`K` 扫 1~5）
- 候选 B：直接 `1 / bias[i]` 反比（卡越多权重越低）
- 候选 C：`max(0, threshold - bias[i])` 缺口越大权重越高

公式形如（具体由实现者定）：

```
low_count_youqing = Σ youqing[at_trains[j]] × weight(bias[j])
weight(b) = (b <= 1 ? K : 1)            // 或 1/b.max(1)、或 max(0, threshold-b)
```

**这一项是当前 build 自适应问题的核心修复**——让速度向 vs 智力向卡组在同一组地区下，因为"自己训练位卡少 → 优先选能覆盖该位的地区"而产生差异。

### 1.4 验收要点

- `test_region_build_sensitivity`：速度向 vs 智力向卡组选**不同**第3年地区组合（**目前 false**）
- 新指标单调性：固定卡组跑 300 局矩阵，新指标高的组合显著好于新指标低的（p < 0.05）
- 不破坏现状：第1/2年 + 速/智卡组，新指标引入对前3名排序影响 < 5%
- `low_count_youqing_bonus` 的 `K`（或 `threshold`）作为新超参，单独扫 1/3/5/8

---

## 2. 第三年体力门禁

### 2.1 核心结论

**吃面那一回合训练必成——这回合体力门禁可以放掉；下一回合没面可吃时训练会失败——这回合必须保留门禁**。把"全局单一阈值"换成"回合级差异化阈值"，并加 RamenSelect 阶段硬底线兜底。

### 2.2 改造思路（双保险，按推荐方案直接落地）

#### A. Train 阶段：拆分 `vital_rest` 为两个阈值

```
vital_rest_eating = 0     // 吃面回合：失败率=0，放掉
vital_rest_normal = 30    // 不吃面回合：失败率 >0，保留
```

守门 2（`policy.rs::decide_train`）按 `game.ramen.current_ramen.is_some()` 选阈值。**为什么不直接用单一全局阈值**：那样在吃面回合会把"低体力但价值高的训练"挡掉（例如 turn 65 体力 12 但彩圈训练 +30 属性）。

#### B. RamenSelect 阶段：兜底"训练后会崩盘"

**`y3_post_train_hard_floor = 15`**：非智训练后体力 < 15 → 候选 `score = NEG_INFINITY`（直接禁面）。15 体力对应大多数训练失败率 < 25%；<15 则失败率通常 > 40%。**智力训练豁免**（智力训练通常回体力，硬底线极少触发）。

**`y3_pre_train_vital_target = 25`**：吃面前希望保留的体力（软目标）。

**`y3_vital_shortfall_weight = 0.5`**：缺口每 1 点的策略评分成本（软成本，不阻断）。以 `pre_vital = 10` 为例，缺口 `15 × 0.5 = 7.5 分`，只起轻微抑制。

**`y3_recovery_horizon = true`**（保留）：turn ≥ 70 关闭 post 缺口惩罚；turn 71 有马纪念后固定 +40 / turn 72+ 超级拉面每回合 +20 由游戏机制吸收。

#### C. preset 推荐值汇总

| 字段 | 当前 | 推荐 | 说明 |
|---|---|---|---|
| `policy.vital_rest_eating` | 不存在 | `0` | 新字段，吃面回合阈值 |
| `policy.vital_rest_normal` | 不存在 | `30` | 新字段，不吃面回合阈值 |
| `y3_post_train_hard_floor` | `0` | `15` | 训练后体力硬底线（非智） |
| `y3_pre_train_vital_target` | `0` | `25` | 吃面前软目标 |
| `y3_vital_shortfall_weight` | `0.0` | `0.5` | 缺口软成本 |
| `y3_recovery_horizon` | `true` | `true` | 保留 |

`y3_post_train_hard_floor` 在 preset 阶段可扫 10/15/20 三档验证。`vital_rest_normal = 30` 与 Y1/Y2 一致。

### 2.3 风险与窗口保护

- **风险场景**：吃面回合把体力打到 ≤ 5 → 下一回合没面吃 → 训练失败
- **窗口保护**：turn 65 彩圈训练 +30 属性的高价值窗口，**不能**被新门禁阻断（验证 `vital_rest_normal = 30` 是否合理——Y1/Y2 也是 30，跨年一致）
- **turn ≥ 70 不退化**：`_recovery_horizon` 接管，超级拉面回合路径不下降

### 2.4 验收手段

300 局同种子矩阵基线对比 + 用现有 `data_collection/ramen_low_score_diagnostic.rs`（支持 `DIAG_REPLAY_IDX` 单局重放）定位"低体力崩盘"的具体回合——比直接看平均分更直接。

---

## 3. `matrix_variant` DSL 改造（lexopt 方案）

### 3.1 核心结论

**保留 DSL**——保留目标不是"重写"，是"结构化"。用 `lexopt` 重写，把"token 表"从"控制流"里抽出来，新增字段只改一张表。

理由：

- `data_collection/skill_pt_matrix.rs` / `skill_pt_phase_matrix.rs` 仍在用 `VARIANT` 环境变量 + shell 循环的网格扫描模式
- token 串本身就是实验"配方名"，进 CSV / 日志都是天然的实验标识符
- 现状 120 行 `else if` 链行为稳定——重写要保证回归测试全过

### 3.2 lexopt 方案

**复用 `lexopt::Parser::from_args`**（项目 CLI 标准库，`bench_base` / `bench_compositions` 全用），不引入 `winnow` 新依赖。理由：

- `winnow` 当前不在 `Cargo.toml`，沙箱 `/home/islab/.cargo/registry` 只读，无法 `cargo add`
- DSL 真正复杂的部分（`friendcap025` 的"必须 3 位数字"）用一个 `if digits.len() != 3` 守卫就够，**不值得为它换整套 combinator 体系**

#### 核心数据结构（字段含义）

**`TokenSpec`**（每行代表一个 token 怎么解析 + 怎么写回配置）：

| 字段 | 含义 |
|---|---|
| `key: &'static str` | token 前缀（"pt" / "sac" / "window" / ...） |
| `target: TokenTarget` | 写到哪个字段（枚举：`PolicyPtRate` / `LocalMaxSacrifice` / `LocalHighFailPenalty` / `LocalFeelingOverflowThreshold` / ... + `Bool(BoolField)` 表示纯开关） |
| `scale: Scale` | 数值处理方式（`Direct` 直接写 / `DivBy100` ÷100 / `FriendCap3Digits` 特化校验三数字） |
| `required: RequiredFlag` | 是否计入 4-flag 必现校验（`Pt` / `Sac` / `M` / `Fail` 四种之一，或 `None`） |
| `apply(&self, value, &mut policy, &mut local, &mut flags)` | 真正写入对应字段 |

**`RequiredFlags`**（替换 `(p, s, m, f)` 元组，字段名自带文档）：

| 字段 | 含义 |
|---|---|
| `pt: bool` | `pt<num>` 必须出现 |
| `sac: bool` | `sac<num>` 必须出现 |
| `m: bool` | `plain` / `long` / `base` 三选一必须出现 |
| `fail: bool` | `fail<num>` 必须出现 |

`flags.check()`：四个 flag 全为 true 才合法，缺一即 `anyhow::bail!("矩阵变体字段不完整: {name}")`。

**`TOKEN_SPECS`**：一张 `const` 数组，包含全部现有 token（约 30 项：4 个必现 + 25 个非必现）。这是**单一数据源**——新增字段只改这张表 + 扩 `TokenTarget` 枚举（编译器穷举校验 `apply` 漏写）。

#### 解析流程

1. 把 token 串 `name` 用 `-` 切分，包成 `vec![OsString("matrix"), OsString(token_1), ...]` 喂给 `lexopt::Parser::from_args`
2. 循环 `parser.next()?`，拿 `Arg::Long(token)`（短横线串里的 token 都被识别为 `--xxx` 形式）
3. **`split_token(&token)` 切分 key/value**：找第一个 ASCII 数字或 `.` 的位置 → 返回 `(&str, Option<&str>)`，例如 `("pt", "16")` / `("rawfail", None)`
4. 在 `TOKEN_SPECS` 线性查找匹配 `key` 的 `TokenSpec`；找不到 → `anyhow::bail!("未知矩阵变体字段: {key}")`
5. 调用 `spec.apply(value, &mut policy, &mut local, &mut flags)`——`apply` 内部按 `scale` 处理（`Direct` / `DivBy100` / `FriendCap3Digits` 特化），并设置对应 `required` flag
6. 循环结束后 `flags.check()`，通过则 `LocalRamenTrainer::with_configs(policy, local)`

`FriendCap3Digits` 特化：解析失败时 `anyhow::bail!("friendcap 必须是三个数字，如 135: {v}")`，通过则写 `[d0, d1, d2]` 三元组到 `friend_outing_cumulative_caps`，并在 `apply` 内做单调 + ≤5 校验（与现状一致）。

#### 关键收益

- **新增字段**：只改 `TOKEN_SPECS` 表 + 扩 `TokenTarget` 枚举；`apply` 漏写触发编译器 `non-exhaustive match` 警告
- **错误信息**：定位到 `key` 而不是整 token 串，调试更快
- **`÷100` 处理**：集中在 `Scale::DivBy100` 分支，避免每个 `else if` 重复写
- **`RequiredFlags`**：结构体字段名代替元组位置，调用点不用记顺序

### 3.3 验收手段

- `skill_pt_matrix` 全部现有 variant 名必须仍能解析（**回归测试必须全过**——这是行为不变重构的唯一硬约束）
- 新增单元测试：`test_matrix_variant_all_tokens_resolve`（每个 TOKEN_SPECS 项都能解析合法值）+ `test_matrix_variant_required_flag_missing`（缺 `pt`/`sac`/`m`/`fail` 任一必报错）
- 新增单元测试：`test_matrix_variant_friendcap_invalid`（非 3 位数字必报错）

---

## 4. 改动清单总览

| 改动 | 风险 | 验收手段 |
|---|---|---|
| ① 地区策略 3 个新指标 | 中（新打分可能破坏现状前 3 名） | 300 局矩阵 + `test_region_build_sensitivity` |
| ② 第三年回合级差异化门禁 | 低（仅改 preset 数值 + 守门 2 拆分） | 300 局矩阵平均分 + 单局崩盘重放 |
| ③ `matrix_variant` lexopt 重构 | 低（行为不变，仅重构） | `skill_pt_matrix` 全部现有 variant 回归 |

实施顺序建议：先 ②（最低风险，只动 preset 数值），再 ①（核心问题，build 自适应），最后 ③（纯重构，可独立合并）。

---

## 附录 A. 现状摘要（写方案前对齐的事实）

| 维度 | 当前状态 | 来源 |
|---|---|---|
| `score_region` 公式 | `bias_sum × (xunlian × 40 + youqing × 1) + pt_bonus × 30 + hint_count × 15` | `policy.rs:607` |
| `bias_sum` | `Σ at_trains[i]` 对应卡组的卡数（`max(0.5)` 下限） | `policy.rs:612-628` |
| 第 3 年地区 | 120 组合全枚举；`xunlian` 恒为 0；`youqing` 权重仅 1.0 | `policy.rs:343`、`gamedata/ramen.rs::RegionEffect` |
| 诀窍机制 | `feeling_stock[3]` 上限 `FEELING_LIMIT=10`；`feeling_slot[3]` 满 `GAUGE_LIMIT=7` 清零 +1 诀窍；溢出按 `feeling_queue` 队首丢弃 | `rules.rs:12,14,88,108` |
| 配方 | `region_feeling[feeling_idx]: [i32;3]` 总和 `RAMEN_COST=5`；第3年地区复用第1年配方 `recipe_idx % region_feeling.len()` | `rules.rs:16,174-183` |
| 吃面 PT | `gain_pt_base[year] + gain_pt_delta[year] × min(eat_count, 5)` | `rules.rs:324` |
| 地区词条档位 | `calc_region_bonus(scenario_pt)` 每 300 PT 一档（最高 5 档 = 10 加成）| `rules.rs:448-452` |
| 第3年体力门禁（preset） | **全部为 0 / false**：`y3_pre_train_vital_target=0`、`y3_post_train_vital_target=0`、`y3_vital_shortfall_weight=0.0`、`y3_post_train_hard_floor=0`、`y3_recovery_horizon=true`（"turn ≥ 70 不再为训练后低体力付费"） | `local_ramen_trainer.rs:1262-1266` |
| 第 1/2 年体力门禁 | `policy.vital_rest`：Y1=30、Y2=30、Y3=0 | `local_ramen_trainer.rs:1277` |
| `matrix_variant` DSL | 短横线分隔的 token 串，4-flag 必现校验（`p/s/m/f`），手写 `else if` 链解析 | `local_ramen_trainer.rs:306-424` |
| DSL 使用现状 | `data_collection/skill_pt_matrix.rs` / `skill_pt_phase_matrix.rs` 仍在用；新一批矩阵已迁移到 `RecommendedRamenTrainer::with_*_overrides` builder | `tools/data_collection/*.rs` |

---

## 附录 B. 第三年体力门禁的领域事实

| 回合 | 训练失败率 | 体力恢复 | 来源 |
|---|---|---|---|
| 第3年普通回合（turn 48-71），**吃面** | 基础失败率 × (100 - 100) % = **0%（必成）** | 训练扣/补按 `calc_training_value` | `scenario_ramen.json:65` `fail_rate_drop: 100`、`effects.rs:200-201`（仅 `eating = current_ramen.is_some()` 生效） |
| 第3年普通回合（turn 48-71），**不吃面** | 基础失败率 × 100%（不变） | 同上 | 同上——`eating=false` 时整段 `if eating` 跳过 |
| turn 71（Y3 有马纪念，赛后） | — | **固定 +40** | `ramen_story_flow.md` / 玩家经验 |
| turn 72-77（超级拉面回合） | **0%**（常驻 `fail_rate_drop=100`）| 每回合开始 **+20** | `effects.rs:102-108` 注释"按最高档位生效" |

**为什么"一并取消"会出问题**：

```
turn 65（Y3）：体力 35，吃面训练必成，扣 30 → turn 65 末体力 5
turn 66（Y3）：体力 5，无可做面（库存/隐藏风味不足），训练失败率 40%
              → 训练实际失败！属性 0 收益 + 失败惩罚
              旧 preset 没有 y3_post_train_hard_floor 兜底 → 直接打空
```

也就是说：**当前 preset 容忍"吃面把体力打空 → 下一回合没面吃 → 训练失败"**——这是 §2 要消除的风险。