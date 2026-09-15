//! 754 维拉面教师数据 → npy 矩阵转换工具（CI 数据分发用）
//!
//! 用法: `convert754 <input_dir> <output_dir>`
//!
//! 递归收集 `<input_dir>` 下所有 `*.bin`（bincode 序列化的 `Vec<RamenTrainingSample>`，
//! 即采样器 shard，每 part 32 条），流式合并输出一套矩阵文件：
//!
//! - `features.npy`   `[N, INPUT_DIM]` f32 —— 决策点特征矩阵（含五维/成长率/上限/技能pt/剧本pt）
//! - `legal_mask.npy` `[N, POLICY_DIM]` f32 —— 合法动作格位掩码（复用 [`RamenTrainingSample::legal_mask`]）
//! - `score_mean.npy` `[N, POLICY_DIM]` f32 —— 各格位候选结算评分均值（无候选格位 NaN）
//! - `score_n.npy`    `[N, POLICY_DIM]` u32 —— 各格位成功 rollout 总次数
//! - `meta.jsonl`     每行一条样本元信息与评分摘要
//! - `summary.json`   数据集概览（样本数/平均分/分布），供快速查看数据质量
//!
//! 评分口径：候选评分 = `sum / n`（成功 rollout 的原始 f64 结算评分均值）；
//! 格位评分 = 该格位所有候选评分的平均；样本平均分 = 所有候选评分的平均。

use std::env;
use std::fs::{self, File};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};

use anyhow::{bail, ensure, Context, Result};
use umasim::game::ramen::features::INPUT_DIM;
use umasim::game::ramen::policy_schema::POLICY_DIM;
use umasim::game::ramen::training_sample::{RamenTrainingSample, SAMPLE_FORMAT_VERSION};

fn main() -> Result<()> {
    let mut args = env::args().skip(1);
    let (input_dir, output_dir) = match (args.next(), args.next()) {
        (Some(i), Some(o)) if o != "-" => (PathBuf::from(i), PathBuf::from(o)),
        _ => bail!("用法: convert754 <input_dir> <output_dir>")
    };

    let mut bins = Vec::new();
    collect_bins(&input_dir, &mut bins)
        .with_context(|| format!("扫描输入目录失败: {}", input_dir.display()))?;
    bins.sort();
    ensure!(!bins.is_empty(), "输入目录下没有找到任何 .bin: {}", input_dir.display());
    eprintln!("发现 {} 个 bin 分片", bins.len());

    // [N, DIM] 行式连续缓冲
    let mut features: Vec<f32> = Vec::new();
    let mut legal: Vec<f32> = Vec::new();
    let mut score_mean: Vec<f32> = Vec::new();
    let mut score_n: Vec<u32> = Vec::new();
    fs::create_dir_all(&output_dir).with_context(|| format!("建输出目录失败: {}", output_dir.display()))?;
    let mut jsonl = BufWriter::new(File::create(output_dir.join("meta.jsonl.tmp"))?);

    let mut sample_scores: Vec<f64> = Vec::new();
    let mut turn_min = i32::MAX;
    let mut turn_max = i32::MIN;
    let mut total_success_rollouts = 0u64;
    let mut total_samples = 0usize;

    for bin in &bins {
        let file = File::open(bin).with_context(|| format!("打开失败: {}", bin.display()))?;
        let samples: Vec<RamenTrainingSample> = bincode::deserialize_from(std::io::BufReader::new(file))
            .with_context(|| format!("bincode 反序列化失败: {}", bin.display()))?;
        eprintln!("  {} -> {} 条样本", bin.display(), samples.len());

        for s in &samples {
            ensure!(
                s.format_version == SAMPLE_FORMAT_VERSION,
                "样本 format_version={} 与当前 {SAMPLE_FORMAT_VERSION} 不符: {}",
                s.format_version,
                bin.display()
            );
            ensure!(
                s.features.len() == INPUT_DIM,
                "特征维度 {} != INPUT_DIM {INPUT_DIM}: {}",
                s.features.len(),
                bin.display()
            );

            features.extend_from_slice(&s.features);
            legal.extend_from_slice(&s.legal_mask());

            // 格位评分聚合：候选评分 = sum/n，格位评分 = 该格位候选平均
            let mut slot_sum = [0.0f64; POLICY_DIM];
            let mut slot_cnt = [0u32; POLICY_DIM];
            let mut slot_n = [0u32; POLICY_DIM];
            let mut cand_means: Vec<f64> = Vec::with_capacity(s.candidates.len());
            let mut n_success = 0u64;
            for c in &s.candidates {
                if c.n > 0 {
                    cand_means.push(c.sum / f64::from(c.n));
                    n_success += u64::from(c.n);
                }
                for &slot in c.slots.as_slice() {
                    ensure!(slot < POLICY_DIM, "policy 格位越界: {slot} >= {POLICY_DIM}");
                    slot_sum[slot] += c.sum / f64::from(c.n.max(1));
                    slot_cnt[slot] += 1;
                    slot_n[slot] += c.n;
                }
            }

            let mut mean_row = vec![f32::NAN; POLICY_DIM];
            for (i, &cnt) in slot_cnt.iter().enumerate() {
                if cnt > 0 {
                    mean_row[i] = (slot_sum[i] / f64::from(cnt)) as f32;
                }
            }
            score_mean.extend_from_slice(&mean_row);
            score_n.extend_from_slice(&slot_n);

            if !cand_means.is_empty() {
                let mean = cand_means.iter().sum::<f64>() / cand_means.len() as f64;
                sample_scores.push(mean);
            }
            total_success_rollouts += n_success;
            turn_min = turn_min.min(s.meta.turn);
            turn_max = turn_max.max(s.meta.turn);

            let mean_json = cand_means.is_empty()
                .then(|| "null".to_string())
                .unwrap_or_else(|| format!("{:.4}", cand_means.iter().sum::<f64>() / cand_means.len() as f64));
            writeln!(
                jsonl,
                "{{\"index\":{},\"turn\":{},\"stage\":{},\"root_seed\":{},\"n_candidates\":{},\"n_success_rollouts\":{},\"mean_score\":{}}}",
                s.meta.index,
                s.meta.turn,
                s.meta.stage,
                s.meta.root_seed,
                s.candidates.len(),
                n_success,
                mean_json
            )?;
            total_samples += 1;
        }
    }
    jsonl.flush()?;

    let n_scored = sample_scores.len();
    let mut sorted = sample_scores.clone();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let mean = if n_scored > 0 { sample_scores.iter().sum::<f64>() / n_scored as f64 } else { f64::NAN };
    let var = if n_scored > 1 {
        sample_scores.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / (n_scored - 1) as f64
    } else {
        0.0
    };
    let pick = |q: f64| -> f64 {
        if n_scored == 0 {
            return f64::NAN;
        }
        sorted[((sorted.len() as f64 - 1.0) * q).round() as usize]
    };

    let summary = serde_json::json!({
        "format_version": SAMPLE_FORMAT_VERSION,
        "input_dim": INPUT_DIM,
        "policy_dim": POLICY_DIM,
        "bins": bins.len(),
        "samples_total": total_samples,
        "samples_with_score": n_scored,
        "total_success_rollouts": total_success_rollouts,
        "score_mean": mean,
        "score_std": var.sqrt(),
        "score_min": sorted.first().copied().unwrap_or(f64::NAN),
        "score_p50": pick(0.50),
        "score_p90": pick(0.90),
        "score_max": sorted.last().copied().unwrap_or(f64::NAN),
        "turn_min": if total_samples > 0 { turn_min } else { 0 },
        "turn_max": if total_samples > 0 { turn_max } else { 0 }
    });

    write_npy_f32(&output_dir.join("features.npy"), &features, total_samples, INPUT_DIM)?;
    write_npy_f32(&output_dir.join("legal_mask.npy"), &legal, total_samples, POLICY_DIM)?;
    write_npy_f32(&output_dir.join("score_mean.npy"), &score_mean, total_samples, POLICY_DIM)?;
    write_npy_u32(&output_dir.join("score_n.npy"), &score_n, total_samples, POLICY_DIM)?;
    fs::rename(output_dir.join("meta.jsonl.tmp"), output_dir.join("meta.jsonl"))
        .context("落盘 meta.jsonl 失败")?;
    serde_json::to_writer_pretty(BufWriter::new(File::create(output_dir.join("summary.json"))?), &summary)?;

    println!(
        "convert754 完成: {} bins / {} 样本 / 平均分 {:.1} / 输出 {}",
        bins.len(),
        total_samples,
        mean,
        output_dir.display()
    );
    Ok(())
}

fn collect_bins(dir: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
    for e in fs::read_dir(dir).with_context(|| format!("读目录失败: {}", dir.display()))? {
        let p = e?.path();
        if p.is_dir() {
            collect_bins(&p, out)?;
        } else if p.extension().and_then(|s| s.to_str()) == Some("bin") {
            out.push(p);
        }
    }
    Ok(())
}

/// npy v1.0 文件头（总长按 64 字节对齐）
fn npy_bytes(descr: &str, rows: usize, cols: usize) -> Vec<u8> {
    let dict = format!("{{'descr': '{descr}', 'fortran_order': False, 'shape': ({rows}, {cols}), }}");
    let pad = (64 - (10 + dict.len() + 1) % 64) % 64;
    let mut header = dict;
    header.extend(std::iter::repeat(' ').take(pad));
    header.push('\n');

    let mut out = vec![0x93u8, b'N', b'U', b'M', b'P', b'Y', 1, 0];
    out.extend_from_slice(&(header.len() as u16).to_le_bytes());
    out.extend_from_slice(header.as_bytes());
    out
}

fn write_npy_f32(path: &Path, data: &[f32], rows: usize, cols: usize) -> Result<()> {
    ensure!(data.len() == rows * cols, "f32 数据量 {} != {}x{}", data.len(), rows, cols);
    let mut w = BufWriter::with_capacity(1 << 20, File::create(path)?);
    w.write_all(&npy_bytes("<f4", rows, cols))?;
    for chunk in data.chunks(1 << 16) {
        let mut buf = Vec::with_capacity(chunk.len() * 4);
        for v in chunk {
            buf.extend_from_slice(&v.to_le_bytes());
        }
        w.write_all(&buf)?;
    }
    w.flush()?;
    Ok(())
}

fn write_npy_u32(path: &Path, data: &[u32], rows: usize, cols: usize) -> Result<()> {
    ensure!(data.len() == rows * cols, "u32 数据量 {} != {}x{}", data.len(), rows, cols);
    let mut w = BufWriter::with_capacity(1 << 20, File::create(path)?);
    w.write_all(&npy_bytes("<u4", rows, cols))?;
    for chunk in data.chunks(1 << 16) {
        let mut buf = Vec::with_capacity(chunk.len() * 4);
        for v in chunk {
            buf.extend_from_slice(&v.to_le_bytes());
        }
        w.write_all(&buf)?;
    }
    w.flush()?;
    Ok(())
}
