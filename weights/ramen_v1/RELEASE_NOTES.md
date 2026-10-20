# NN 权重 ramen_v1（首版拉面杯 NN 蒸馏）

## 一句话
754 维输入 → 245 维输出的拉面杯 NN 策略/价值网络，从 GBDT 决策样本蒸馏而来，供 `ramen_root_bench --rollout-model` 与手写/GBDT leaf 基线做端到端对比。

## 数据
- 数据集：`data754/prod_n4096`（search_n=4096，record_ordered_rollouts=true）
- 采样空间指纹：`6c30529e9333fb94`（== GEN1_SPACE_HASH_V1，可用当前代码导出）
- 决策样本：1527（训练 1375 / 验证 152）

## 模型
- 输入 `input_dim=754`（global 154 + cards 6×35 + persons 13×30）
- 输出 `output_dim=245`（policy 234 + choice 8 + value 3）
- 参数 `308917`（约 31 万）
- 结构：token_dim=64 / heads=4 / encoder_blocks=1 / mlp_width=192 / attention_kind=simple
- ONNX opset=13，max_abs_error=5.96e-07

## value 归一化（ValueNormJson，推理端反归一化用）
- center = [64250.469, 2110.318, 65702.718]
- scale  = [2684.108, 733.344, 2891.627]
- 反归一化：`center[i] + scale[i] * output[i]`，stdev 路 `.max(0.0)`

## 产物清单
- model.onnx（1.2M，核心推理权重，RamenNnTrainer::load 加载）
- model.onnx.json（1.1K，旁车元信息，含 value_normalization）
- best.pt（3.6M，PyTorch 训练 checkpoint，供后续重训/蒸馏）
- run.json / meta.json / labels.json（训练元信息 + 标签）

## 训练入口
`.github/workflows/ramen-nn-train.yml`（feat/nn-train 分支）；
脚本 `scripts/ramen_nn/{labels,train,export_onnx}.py`。

## 状态
首版，未 bench。下一步接入 `ramen_root_bench` 跑 NN vs 手写 leaf 对比。