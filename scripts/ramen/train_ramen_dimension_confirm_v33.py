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
    columns = [matrix]
    needed = target_dimension - matrix.shape[1]
    made = 0
    lag = 1
    while made < needed:
        take = min(matrix.shape[1], needed - made)
        columns.append((matrix[:, :take] * np.roll(matrix, -lag, axis=1)[:, :take]).astype(np.float32))
        made += take
        lag += 1
    return np.concatenate(columns, axis=1)[:, :target_dimension]


candidates = {
    "S161-H512": (np.asarray([row["s"] for row in rows], dtype=np.float32), 512),
    "MID293-H128": (np.asarray([row["mid"] for row in rows], dtype=np.float32), 128),
    "L398-H512": (large, 512),
    "X768-H512": (expand_to(large, 768), 512),
    "X1536-H64": (expand_to(large, 1536), 64),
}
seeds = [3301, 3302, 3303, 3304, 3305]


def adam_step(parameter, gradient, first, second, step, learning_rate):
    gradient = np.clip(gradient, -1.0, 1.0)
    first *= 0.9
    first += 0.1 * gradient
    second *= 0.999
    second += 0.001 * gradient * gradient
    corrected_first = first / (1.0 - 0.9**step)
    corrected_second = second / (1.0 - 0.999**step)
    parameter -= learning_rate * corrected_first / (np.sqrt(corrected_second) + 1e-8)


print("# 拉面杯神经网络维度多种子确认 v33\n")
print(
    f"> {len(groups)} 个根局面（训练 {len(training_groups)} / 验证 {len(validation_groups)}），"
    "每候选 64 次完整终局 rollout；每个结构使用 5 个训练种子。\n"
)
print("> 修复 v32 数值问题：按训练集标准化后裁剪到 [-8,8]，使用 Adam、梯度裁剪和权重衰减。\n")
print("|结构|参数量|Top-1均值±标准差|后悔值均值±标准差|P90后悔值|MAE均值|发散次数|")
print("|---|---:|---:|---:|---:|---:|---:|")

for name, (original, width) in candidates.items():
    mean = original[training_indices].mean(0)
    std = original[training_indices].std(0)
    std[std < 1e-4] = 1.0
    features = np.clip((original - mean) / std, -8.0, 8.0).astype(np.float32)
    metrics = []
    diverged = 0
    for seed in seeds:
        random = np.random.default_rng(seed + features.shape[1] * 13 + width)
        weight1 = random.normal(0, math.sqrt(2 / features.shape[1]), (features.shape[1], width)).astype(np.float32)
        bias1 = np.zeros(width, np.float32)
        weight2 = random.normal(0, math.sqrt(2 / width), (width, 1)).astype(np.float32)
        bias2 = np.zeros(1, np.float32)
        parameters = [weight1, bias1, weight2, bias2]
        first = [np.zeros_like(parameter) for parameter in parameters]
        second = [np.zeros_like(parameter) for parameter in parameters]
        step = 0
        for epoch in range(100):
            order = random.permutation(training_indices)
            learning_rate = 0.001 if epoch < 55 else 0.0003 if epoch < 80 else 0.0001
            for start in range(0, len(order), 512):
                indices = order[start : start + 512]
                batch = features[indices]
                expected = target[indices]
                preactivation = batch @ weight1 + bias1
                activation = np.maximum(preactivation, 0)
                prediction = activation @ weight2 + bias2
                derivative = 2 * (prediction - expected) / len(indices)
                gradients = [
                    batch.T @ ((derivative @ weight2.T) * (preactivation > 0)) + 1e-5 * weight1,
                    ((derivative @ weight2.T) * (preactivation > 0)).sum(0),
                    activation.T @ derivative + 1e-5 * weight2,
                    derivative.sum(0),
                ]
                step += 1
                for parameter, gradient, m, v in zip(parameters, gradients, first, second):
                    adam_step(parameter, gradient, m, v, step, learning_rate)
        prediction = (np.maximum(features @ weight1 + bias1, 0) @ weight2 + bias2)[:, 0] * 1000
        if not np.isfinite(prediction).all() or np.max(np.abs(prediction)) > 20000:
            diverged += 1
            continue
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
        metrics.append((wins / len(validation_groups), np.mean(regrets), np.percentile(regrets, 90), np.mean(errors)))
    parameter_count = features.shape[1] * width + width + width + 1
    if metrics:
        values = np.asarray(metrics)
        print(
            f"|{name}|{parameter_count:,}|{values[:,0].mean():.1%} ± {values[:,0].std():.1%}|"
            f"{values[:,1].mean():.1f} ± {values[:,1].std():.1f}|{values[:,2].mean():.1f}|"
            f"{values[:,3].mean():.1f}|{diverged}/{len(seeds)}|"
        )
    else:
        print(f"|{name}|{parameter_count:,}|—|—|—|—|{diverged}/{len(seeds)}|")

print("\n选择顺序：先排除发散结构，再以平均后悔值为主、P90与跨种子标准差为辅；Top-1仅作参考。")
