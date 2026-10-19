"""训练经典 GBDT 价值代理 + 导出树 JSON 供 Rust 推理器使用。

用经典 GradientBoostingRegressor（标准 tree_ 结构，threshold 是实际值）而非
HistGBDT（阈值是分箱值，Rust 集成要额外还原 binning）。取 R² 与 Hist 相近、却
能直接序列化给 Rust 的折中。
"""
from __future__ import annotations

import json
from pathlib import Path

import numpy as np
from sklearn.ensemble import GradientBoostingRegressor
from sklearn.metrics import mean_absolute_error, r2_score
from sklearn.model_selection import train_test_split

DATA_DIR = Path("training_data_npy")
OUT = DATA_DIR / "gbdt_model.json"

# 经典 GBDT 单核较慢，用验证过的 200 树 d4（R²≈0.81）而非 300 d5（会超时）。
N_EST = 200
MAX_DEPTH = 4
LR = 0.1


def load() -> tuple[np.ndarray, np.ndarray]:
    X = np.load(DATA_DIR / "features.npy", mmap_mode="r")
    y: list[float] = []
    with (DATA_DIR / "meta.jsonl").open(encoding="utf-8") as f:
        for line in f:
            if line.strip():
                y.append(json.loads(line)["mean_score"])
    return X, np.asarray(y, dtype=np.float64)


def tree_predict(x: np.ndarray, trees: list, lr: float, init: float) -> float:
    s = init
    for t in trees:
        feat, thr = t["feature"], t["threshold"]
        left, right, val = t["left"], t["right"], t["value"]
        node = 0
        while feat[node] >= 0:  # 叶子 feature = -2（sklearn TREE_UNDEFINED）
            node = left[node] if x[feat[node]] <= thr[node] else right[node]
        s += lr * val[node]
    return s


def main() -> None:
    X, y = load()
    n = len(X)
    print(f"样本 {n} 特征 {X.shape[1]} 分数 [{y.min():.0f},{y.max():.0f}] 均值 {y.mean():.0f}")

    idx = np.arange(n)
    tr, te = train_test_split(idx, test_size=0.2, random_state=42)
    Xtr, Xte, ytr, yte = X[tr], X[te], y[tr], y[te]
    print(f"训练 {len(tr)} / 测试 {len(te)}")

    gb = GradientBoostingRegressor(
        n_estimators=N_EST, max_depth=MAX_DEPTH, learning_rate=LR,
        subsample=0.8, random_state=42,
    )
    gb.fit(Xtr, ytr)
    pred = gb.predict(Xte)
    print(f"GBDT({N_EST},d{MAX_DEPTH}) MAE={mean_absolute_error(yte, pred):.0f}  R²={r2_score(yte, pred):+.4f}")

    # 用全部数据重训一遍再导出（留出集只用于报分，导出的树要吃到全量）
    gb_full = GradientBoostingRegressor(
        n_estimators=N_EST, max_depth=MAX_DEPTH, learning_rate=LR,
        subsample=0.8, random_state=42,
    )
    gb_full.fit(X, y)
    trees_full = []
    for est in gb_full.estimators_.ravel():
        t = est.tree_
        trees_full.append({
            "feature": t.feature.astype(np.int32).tolist(),
            "threshold": t.threshold.astype(np.float32).tolist(),
            "left": t.children_left.astype(np.int32).tolist(),
            "right": t.children_right.astype(np.int32).tolist(),
            "value": t.value[:, 0, 0].astype(np.float64).tolist(),
        })
    init_full = float(np.asarray(gb_full.init_.constant_).reshape(-1)[0])
    model_full = {
        "format_version": 1,
        "init": init_full,
        "learning_rate": gb_full.learning_rate,
        "n_trees": len(trees_full),
        "trees": trees_full,
    }
    with OUT.open("w", encoding="utf-8") as f:
        json.dump(model_full, f)
    print(f"全量重训后导出 {OUT} (trees={len(trees_full)}, init={init_full:.1f})")

    # 导出无损校验（用全量模型的树，手写遍历 vs sklearn）
    sk = gb_full.predict(X[:8])
    manual = [tree_predict(x, trees_full, gb_full.learning_rate, init_full) for x in X[:8]]
    diffs = [abs(a - b) for a, b in zip(sk, manual)]
    print(f"导出无损校验(前8条): max|diff|={max(diffs):.2e}")


if __name__ == "__main__":
    main()
