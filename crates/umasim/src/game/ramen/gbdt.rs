//! GBDT 价值代理推理器
//!
//! 加载 sklearn `GradientBoostingRegressor` 导出的树 JSON（见
//! `scripts/ramen_nn/gbdt_export.py`），对 754 维 Ramen 特征做标量价值预测，
//! 供 leaf 截断估值使用。语义与 NN 的 value 段（`VALUE_OFF..`）一致：
//! 都是「期望终局分」，口径为真实分数，无需再反归一化。

use anyhow::{Context, Result};
use serde::Deserialize;

/// 单棵回归树的节点数组（按 sklearn `Tree` 的 node-major 布局）
#[derive(Debug, Deserialize)]
pub struct GbdtTree {
    /// 每个节点的分裂特征索引；叶子为 `-2`（sklearn `TREE_UNDEFINED`）
    pub feature: Vec<i32>,
    /// 每个节点的分裂阈值（叶子处无意义，填 0）
    pub threshold: Vec<f32>,
    /// 每个节点的左子下标；叶子处为 `-1`
    pub left: Vec<i32>,
    /// 每个节点的右子下标；叶子处为 `-1`
    pub right: Vec<i32>,
    /// 每个节点的预测值（仅叶子有效）
    pub value: Vec<f64>,
}

/// 完整 GBDT 模型（加性集成：init + lr * Σ leaf_value）
#[derive(Debug, Deserialize)]
pub struct GbdtModel {
    pub format_version: u32,
    /// 初始预测（sklearn DummyRegressor 常数，即训练集均值）
    pub init: f64,
    /// 学习率
    pub learning_rate: f64,
    pub n_trees: usize,
    pub trees: Vec<GbdtTree>,
}

impl GbdtModel {
    /// 从导出 JSON 解析模型
    pub fn from_json(json: &str) -> Result<Self> {
        let model: GbdtModel = serde_json::from_str(json).context("解析 GBDT 模型 JSON 失败")?;
        anyhow::ensure!(
            model.format_version == 1,
            "不支持的 GBDT 模型格式版本 {}",
            model.format_version
        );
        anyhow::ensure!(
            model.trees.len() == model.n_trees,
            "树数量不符: n_trees={} 实际 {}",
            model.n_trees,
            model.trees.len()
        );
        Ok(model)
    }

    /// 对一条 754 维特征做价值预测（旧 sklearn 语义：<= 阈值走左）
    pub fn predict(&self, features: &[f32]) -> f64 {
        let mut sum = self.init;
        for tree in &self.trees {
            let mut node = 0usize;
            while tree.feature[node] >= 0 {
                let f = tree.feature[node] as usize;
                node = if features[f] <= tree.threshold[node] {
                    tree.left[node] as usize
                } else {
                    tree.right[node] as usize
                };
            }
            sum += self.learning_rate * tree.value[node];
        }
        sum
    }
}

#[cfg(test)]
mod tests {
    use super::GbdtModel;

    fn sample_json() -> &'static str {
        // init=5, lr=0.5, 两棵树：
        //   树1：feature0 <= 0.5 ? 10 : 20
        //   树2：feature0 <= 0.5 ? 1  : 2
        // 期望：x<=0.5 → 5 + 0.5*(10+1) = 10.5；x>0.5 → 5 + 0.5*(20+2) = 16.0
        r#"{"format_version":1,"init":5.0,"learning_rate":0.5,"n_trees":2,"trees":[
            {"feature":[0,-2,-2],"threshold":[0.5,0.0,0.0],"left":[1,-1,-1],"right":[2,-1,-1],"value":[0.0,10.0,20.0]},
            {"feature":[0,-2,-2],"threshold":[0.5,0.0,0.0],"left":[1,-1,-1],"right":[2,-1,-1],"value":[0.0,1.0,2.0]}
        ]}"#
    }

    #[test]
    fn parse_and_predict() {
        let m = GbdtModel::from_json(sample_json()).unwrap();
        assert_eq!(m.n_trees, 2);
        assert!((m.predict(&[0.3]) - 10.5).abs() < 1e-9);
        assert!((m.predict(&[0.7]) - 16.0).abs() < 1e-9);
        assert!((m.predict(&[0.5]) - 10.5).abs() < 1e-9);
    }

    #[test]
    fn rejects_bad_version() {
        let bad = r#"{"format_version":9,"init":0.0,"learning_rate":0.1,"n_trees":0,"trees":[]}"#;
        assert!(GbdtModel::from_json(bad).is_err());
    }

    /// 与 `gbdt_export.py` 导出的真值逐条对拍（用真实 gbdt_model.json + 真实特征）。
    ///
    /// 数据文件由 `scripts/ramen_nn/gbdt_export.py` 链路的 `verify_ref.json` 生成：
    /// 10 条 754 维特征 + Python 手写遍历的预测值。Rust 推理器必须逐条精确复现。
    /// 本测试依赖 `training_data_npy/` 下的工件，缺文件时跳过而非报错。
    #[test]
    fn matches_python_reference() {
        let root = crate::utils::get_workspace_root();
        let root = match root {
            Ok(r) => r,
            Err(_) => return,
        };
        let model_path = root.join("training_data_npy").join("gbdt_model.json");
        let ref_path = root.join("training_data_npy").join("verify_ref.json");
        if !model_path.is_file() || !ref_path.is_file() {
            eprintln!("跳过：缺少 gbdt_model.json 或 verify_ref.json");
            return;
        }
        let json = std::fs::read_to_string(&model_path).expect("读 gbdt_model.json");
        let model = GbdtModel::from_json(&json).expect("解析 gbdt_model.json");
        let ref_json = std::fs::read_to_string(&ref_path).expect("读 verify_ref.json");
        let data: serde_json::Value = serde_json::from_str(&ref_json).expect("解析 verify_ref.json");
        let feats = data["features"].as_array().expect("features 数组");
        let py = data["py_predict"].as_array().expect("py_predict 数组");
        assert_eq!(feats.len(), py.len(), "特征与真值条数不符");
        for (i, (fv, pv)) in feats.iter().zip(py.iter()).enumerate() {
            let x: Vec<f32> = fv.as_array().expect("特征数组")
                .iter().map(|v| v.as_f64().expect("f64") as f32).collect();
            let expected = pv.as_f64().expect("真值 f64");
            let got = model.predict(&x);
            let diff = (got - expected).abs();
            assert!(
                diff < 1e-3,
                "[{i}] Rust={got:.6} Python={expected:.6} diff={diff:.2e} 不一致"
            );
        }
        eprintln!("一致性验证通过：{} 条特征 Rust == Python", feats.len());
    }
}
