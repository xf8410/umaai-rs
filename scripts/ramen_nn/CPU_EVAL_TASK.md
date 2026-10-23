# NN 权重「纯 CPU 完整育成」权威验收任务

> 一句话目标：在**纯 CPU、不依赖 GPU** 的条件下，用 Rust 侧完整育成基准，判定 NN 权重是否在权威口径下**超过手写策略**。

---

## 0. 为什么有这个任务

NN 权重（`nn-ramen-v1`）已经训练并导出成 ONNX，但「它到底有没有比手写策略强」目前**还没有权威答案**。

`scripts/ramen_nn/DESIGN.md` 第 179 行定死了口径：

> 最终是否超过手写策略仍只看 Rust 侧纯网络完整育成基准，Python 留出指标不能替代该验收。

这条口径对应的就是本任务。此前它被 GPU 卡住（NN 臂写死 CUDA、CI 无 GPU），现已通过 `--cpu-only` 打通纯 CPU 链路，本任务把它**跑完整、给出结论**。

---

## 1. 权威口径（只有一个，别混）

| 项 | 值 |
|---|---|
| 入口 | `ramen_root_bench`（Rust 固定根实验入口） |
| 模式 | `--mode teacher-game`（完整育成整局） |
| NN 臂 | `--cpu-only`（接 NN rollout、纯 CPU tract 推理） |
| 手写臂 | `--handwritten-rollout`（不接网络、手写 rollout 基策） |

两臂**只有 rollout 基策不同**（NN vs 手写），搜索框架相同。两臂终局得分的差，就是 NN 权重相对手写策略的边际。

> 注意：手写臂不接网络，但 `RamenNnTrainer::load` 在启动时**无条件**读 `--rollout-model`（为拿反归一化元数据），所以手写臂命令里**也必须传 `--rollout-model`**，否则启动失败。这是已知坑，不要省。

---

## 2. 前置状态（已完成）

- 权重 release：`nn-ramen-v1`，assets = `model.onnx` + `model.onnx.json`（反归一化常数）
- `--cpu-only` 已落地（commit `0b5eef9`），`build`（17 单测）+ `smoke`（两臂各一局 `search_n=32`）全绿
- smoke 只证明**链路通**，不回答优劣；单局结果不采信

---

## 3. 具体步骤

### 3.1 拉权重

```bash
mkdir -p /tmp/nn
curl -fL -o /tmp/nn/model.onnx       https://github.com/xf8410/umaai-rs/releases/download/nn-ramen-v1/model.onnx
curl -fL -o /tmp/nn/model.onnx.json  https://github.com/xf8410/umaai-rs/releases/download/nn-ramen-v1/model.onnx.json
```

### 3.2 编译（必须带 onnx feature）

```bash
cargo build --release -p umasim --bin ramen_root_bench --features cli,onnx
```

（`ramen_root_bench` 的 `required-features` 含 `onnx`，而 default 不含，漏了会编译不到。）

### 3.3 跑两臂（CRN 配对）

NN 臂：

```bash
cargo run --release -p umasim --bin ramen_root_bench --features cli,onnx -- \
  --mode teacher-game \
  --rollout-model /tmp/nn/model.onnx \
  --cpu-only \
  --search-n 128 \
  --seed 61444 \
  --plan-index 0
```

手写臂：

```bash
cargo run --release -p umasim --bin ramen_root_bench --features cli,onnx -- \
  --mode teacher-game \
  --rollout-model /tmp/nn/model.onnx \
  --handwritten-rollout \
  --search-n 128 \
  --seed 61444 \
  --plan-index 0
```

### 3.4 逐决策留痕（可选，推荐做）

两臂各加 `--steps-csv`（整局逐步决策实测值）与 `--decision-csv`（逐决策特征），两臂 CSV 可直接**逐行比较**，定位 NN 臂与手写臂在哪个阶段/哪个动作上分叉。

---

## 4. CRN 配对纪律（硬约束）

- 每「一对」= 相同的 `--seed` + 相同的 `--plan-index` + 相同的 `--search-n`，两臂各跑一局，其余参数一致。
- **多局** = 多个不同 `--seed`（每个 seed 是独立一局）；`--run-idx` 保持默认。
- **多马娘/多卡组** = 遍历 `--plan-index`（采样空间里的计划下标，每个下标对应一组马娘 + 卡组 + 形状）。
- 判分**只对比同对的两臂**，不跨对、不跨 seed 混比。

---

## 5. 参数约定

| 参数 | 正式值 | 说明 |
|---|---|---|
| `--search-n` | **128** | 正式口径，勿用 smoke 的 32 |
| `--radical-factor-max` | 1.4 | 教师激进度上限，默认 |
| `--seed` | 61444 起，逐个 +1 | 每局一个独立 seed |
| `--plan-index` | 0..K-1 | 覆盖多种马娘/卡组，K 由拍板定 |
| `--workers` | 默认（逻辑核数） | 纯 CPU，按机器核数吃满 |

---

## 6. 验收判据

- 记每臂每局 `score`（含 Hint 折算）与 `score_pt`（不含 Hint）。
- 主力指标 = **多局平均 `score`**，CRN 配对下算 `d = NN_mean − 手写_mean`。
- 判据带噪声：配对 t 检验（或 z），**显著提升才算 NN 胜**；不显著或为负，不进主线。
- ⚠️ 当前权重是 pilot 版（仅 1527 条数据训练），结论只对 pilot 口径有效；8 万条正式数据到齐后**必须同协议复验**，不能沿用 pilot 结论。

---

## 7. 产出与归档

1. 每局 `score / score_pt / 步数 / 墙钟` 落盘，按臂归档（目录约定见 bench 归档纪律）。
2. 汇总报告（两臂逐局对照表 + 配对显著性 + ValueError 说明）→ `xf8410/uma-algo-reports`。
3. 结论（进主线 / 剔除）归档记忆仓。

---

## 8. 成本与待拍板

- 纯 CPU 下 NN 臂单局 `search_n=32` 约 94s；`search_n=128` 显著更慢。完整验收总量约几十小时。
- **点火前需主人拍板**：跑多少个计划 `K`、每个计划多少个 seed、总局数预算。未拍板不点火。