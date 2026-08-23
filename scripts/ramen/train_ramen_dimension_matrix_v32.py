import collections
import glob
import json
import math

import numpy as np

rows = []
for path in glob.glob("data/*/*.jsonl"):
    with open(path, encoding="utf-8") as file:
        rows.extend(json.loads(line) for line in file if line.strip())

groups = collections.defaultdict(list)
for index, row in enumerate(rows):
    groups[row["group"]].append(index)
all_groups = sorted(groups)
validation_groups = [group for group in all_groups if group % 5 == 0]
training_groups = [group for group in all_groups if group % 5 != 0]
training_indices = np.asarray([index for group in training_groups for index in groups[group]])

target = np.asarray([row["teacher_mean"] for row in rows], dtype=np.float32)
for indices in groups.values():
    target[indices] = (target[indices] - target[indices].mean()) / 1000.0
target = target[:, None]
large = np.asarray([row["l"] for row in rows], dtype=np.float32)


def expand_to(matrix, target_dimension):
    if matrix.shape[1] >= target_dimension:
        return matrix[:, :target_dimension]
    columns = [matrix]
    needed = target_dimension - matrix.shape[1]
    made = 0
    lag = 1
    while made < needed:
        take = min(matrix.shape[1], needed - made)
        interaction = matrix[:, :take] * np.roll(matrix, -lag, axis=1)[:, :take]
        columns.append(interaction.astype(np.float32))
        made += take
        lag += 1
    return np.concatenate(columns, axis=1)[:, :target_dimension]


schemas = {
    "S161": np.asarray([row["s"] for row in rows], dtype=np.float32),
    "MID293": np.asarray([row["mid"] for row in rows], dtype=np.float32),
    "L398": large,
    "X768": expand_to(large, 768),
    "X1121": expand_to(large, 1121),
    "X1536": expand_to(large, 1536),
}

print("# 拉面杯神经网络输入维度 × 隐藏宽度训练矩阵 v32\n")
print(
    f"> {len(groups)} 个独立根局面（训练 {len(training_groups)} / 验证 {len(validation_groups)}），"
    "每候选 32 次完整终局 rollout。架构实验训练，不导出生产模型、不接入策略。\n"
)
print("> X768/X1121/X1536 由 L398 增加确定性的显式二阶交互项，不是补零。\n")
print("|输入方案|输入维度|隐藏宽度|参数量|Top-1|平均后悔值|P90后悔值|相对分数MAE|")
print("|---|---:|---:|---:|---:|---:|---:|---:|")

for name, original in schemas.items():
    mean = original[training_indices].mean(0)
    std = original[training_indices].std(0)
    std[std < 1e-5] = 1
    features = (original - mean) / std
    for width in [64, 128, 256, 512, 1024]:
        random = np.random.default_rng(320000 + features.shape[1] * 7 + width)
        weight1 = random.normal(
            0, math.sqrt(2 / features.shape[1]), (features.shape[1], width)
        ).astype(np.float32)
        bias1 = np.zeros(width, np.float32)
        weight2 = random.normal(0, math.sqrt(2 / width), (width, 1)).astype(np.float32)
        bias2 = np.zeros(1, np.float32)
        learning_rate = 0.0015
        for epoch in range(70):
            order = random.permutation(training_indices)
            for start in range(0, len(order), 512):
                indices = order[start : start + 512]
                batch = features[indices]
                expected = target[indices]
                preactivation = batch @ weight1 + bias1
                activation = np.maximum(preactivation, 0)
                prediction = activation @ weight2 + bias2
                derivative = 2 * (prediction - expected) / len(indices)
                gradient_weight2 = activation.T @ derivative
                gradient_bias2 = derivative.sum(0)
                derivative_hidden = (derivative @ weight2.T) * (preactivation > 0)
                gradient_weight1 = batch.T @ derivative_hidden
                gradient_bias1 = derivative_hidden.sum(0)
                weight1 -= learning_rate * np.clip(gradient_weight1, -5, 5)
                bias1 -= learning_rate * np.clip(gradient_bias1, -5, 5)
                weight2 -= learning_rate * np.clip(gradient_weight2, -5, 5)
                bias2 -= learning_rate * np.clip(gradient_bias2, -5, 5)
            if epoch in (30, 50):
                learning_rate *= 0.3
        prediction = (np.maximum(features @ weight1 + bias1, 0) @ weight2 + bias2)[:, 0] * 1000
        regrets = []
        wins = 0
        errors = []
        for group in validation_groups:
            indices = groups[group]
            truth = np.asarray([rows[index]["teacher_mean"] for index in indices])
            estimated = prediction[indices]
            chosen = int(np.argmax(estimated))
            best = int(np.argmax(truth))
            wins += chosen == best
            regrets.append(float(truth[best] - truth[chosen]))
            errors.extend(abs((truth - truth.mean()) - estimated))
        parameters = features.shape[1] * width + width + width + 1
        print(
            f"|{name}|{features.shape[1]}|{width}|{parameters:,}|"
            f"{wins / len(validation_groups):.1%}|{np.mean(regrets):.1f}|"
            f"{np.percentile(regrets, 90):.1f}|{np.mean(errors):.1f}|"
        )

print("\n本轮是单训练种子宽筛；入围结构再做多训练种子确认。")
