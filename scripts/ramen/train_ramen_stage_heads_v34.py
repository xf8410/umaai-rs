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

STAGES = ["RamenSelect", "SpecialSelect", "Train", "RegionSelect"]
stage_of_row = np.asarray([row["stage"] for row in rows])


def normalize(values):
    mean = values[training_indices].mean(0)
    std = values[training_indices].std(0)
    std[std < 1e-4] = 1.0
    return np.clip((values - mean) / std, -8.0, 8.0).astype(np.float32)


FEATURES = {
    "S161": normalize(np.asarray([row["s"] for row in rows], dtype=np.float32)),
    "MID293": normalize(np.asarray([row["mid"] for row in rows], dtype=np.float32)),
}

# name, features, expert grouping, hidden width. The expert variants keep total first-layer
# parameter count close to S161-H512, so gains are not bought merely by more parameters.
CONFIGS = [
    ("S161-Unified-H512", "S161", {stage: "all" for stage in STAGES}, {"all": 512}),
    (
        "S161-3Expert-H170",
        "S161",
        {"RamenSelect": "ramen", "SpecialSelect": "ramen", "Train": "train", "RegionSelect": "region"},
        {"ramen": 170, "train": 170, "region": 170},
    ),
    (
        "S161-4Expert-H128",
        "S161",
        {stage: stage for stage in STAGES},
        {stage: 128 for stage in STAGES},
    ),
    ("MID293-Unified-H128", "MID293", {stage: "all" for stage in STAGES}, {"all": 128}),
    (
        "MID293-4Expert-H32",
        "MID293",
        {stage: stage for stage in STAGES},
        {stage: 32 for stage in STAGES},
    ),
]
SEEDS = [3401, 3402, 3403]


def train_expert(features, indices, width, seed):
    random = np.random.default_rng(seed)
    input_dim = features.shape[1]
    weight1 = random.normal(0, math.sqrt(2 / input_dim), (input_dim, width)).astype(np.float32)
    bias1 = np.zeros(width, np.float32)
    weight2 = random.normal(0, math.sqrt(2 / width), (width, 1)).astype(np.float32)
    bias2 = np.zeros(1, np.float32)
    params = [weight1, bias1, weight2, bias2]
    first = [np.zeros_like(param) for param in params]
    second = [np.zeros_like(param) for param in params]
    step = 0
    for epoch in range(100):
        order = random.permutation(indices)
        learning_rate = 0.001 if epoch < 55 else 0.0003 if epoch < 80 else 0.0001
        for start in range(0, len(order), 512):
            selected = order[start : start + 512]
            batch = features[selected]
            expected = target[selected]
            preactivation = batch @ weight1 + bias1
            activation = np.maximum(preactivation, 0)
            prediction = activation @ weight2 + bias2
            derivative = 2 * (prediction - expected) / len(selected)
            hidden_derivative = (derivative @ weight2.T) * (preactivation > 0)
            gradients = [
                batch.T @ hidden_derivative + 1e-5 * weight1,
                hidden_derivative.sum(0),
                activation.T @ derivative + 1e-5 * weight2,
                derivative.sum(0),
            ]
            step += 1
            for param, gradient, m, v in zip(params, gradients, first, second):
                gradient = np.clip(gradient, -1.0, 1.0)
                m *= 0.9
                m += 0.1 * gradient
                v *= 0.999
                v += 0.001 * gradient * gradient
                param -= learning_rate * (m / (1 - 0.9**step)) / (np.sqrt(v / (1 - 0.999**step)) + 1e-8)
    return params


def predict(features, params, indices):
    weight1, bias1, weight2, bias2 = params
    return (np.maximum(features[indices] @ weight1 + bias1, 0) @ weight2 + bias2)[:, 0] * 1000


def evaluate(prediction):
    overall_regrets = []
    overall_wins = 0
    stage_regrets = collections.defaultdict(list)
    stage_wins = collections.Counter()
    stage_counts = collections.Counter()
    for group in validation_groups:
        indices = groups[group]
        truth = np.asarray([rows[index]["teacher_mean"] for index in indices])
        estimated = prediction[indices]
        chosen = int(np.argmax(estimated))
        best = int(np.argmax(truth))
        regret = float(truth[best] - truth[chosen])
        stage = rows[indices[0]]["stage"]
        overall_regrets.append(regret)
        overall_wins += chosen == best
        stage_regrets[stage].append(regret)
        stage_wins[stage] += chosen == best
        stage_counts[stage] += 1
    return overall_wins / len(validation_groups), np.mean(overall_regrets), np.percentile(overall_regrets, 90), stage_regrets, stage_wins, stage_counts


print("# 拉面杯统一网络 vs 阶段专家矩阵 v34\n")
print(
    f"> {len(groups)} 个根局面（训练 {len(training_groups)} / 验证 {len(validation_groups)}），"
    "每候选 64 次完整终局 rollout；每结构 3 个训练种子。\n"
)
print("> 专家方案按总参数预算近似配平；同一根局面的候选不会跨训练/验证集。\n")
print("|结构|总参数量|Top-1|平均后悔值|P90|拉面选择后悔|隐藏风味后悔|训练后悔|地区后悔|")
print("|---|---:|---:|---:|---:|---:|---:|---:|---:|")

for config_index, (name, feature_name, stage_map, widths) in enumerate(CONFIGS):
    features = FEATURES[feature_name]
    seed_metrics = []
    parameter_count = sum(features.shape[1] * width + width + width + 1 for width in widths.values())
    for seed in SEEDS:
        models = {}
        prediction = np.zeros(len(rows), dtype=np.float32)
        for expert, width in widths.items():
            expert_stages = [stage for stage, mapped in stage_map.items() if mapped == expert]
            train_indices = np.asarray(
                [index for index in training_indices if stage_of_row[index] in expert_stages], dtype=np.int64
            )
            all_indices = np.asarray(
                [index for index in range(len(rows)) if stage_of_row[index] in expert_stages], dtype=np.int64
            )
            model = train_expert(features, train_indices, width, seed + config_index * 100 + len(models))
            models[expert] = model
            prediction[all_indices] = predict(features, model, all_indices)
        seed_metrics.append(evaluate(prediction))
    top1 = np.asarray([metric[0] for metric in seed_metrics])
    regret = np.asarray([metric[1] for metric in seed_metrics])
    p90 = np.asarray([metric[2] for metric in seed_metrics])
    stage_means = {}
    for stage in STAGES:
        values = []
        for metric in seed_metrics:
            values.extend(metric[3].get(stage, []))
        stage_means[stage] = np.mean(values) if values else float("nan")
    print(
        f"|{name}|{parameter_count:,}|{top1.mean():.1%} ± {top1.std():.1%}|"
        f"{regret.mean():.1f} ± {regret.std():.1f}|{p90.mean():.1f}|"
        f"{stage_means['RamenSelect']:.1f}|{stage_means['SpecialSelect']:.1f}|"
        f"{stage_means['Train']:.1f}|{stage_means['RegionSelect']:.1f}|"
    )

print("\n判定以总体平均后悔值为主，同时要求不能靠牺牲稀有的地区选择阶段换取表面收益。")
