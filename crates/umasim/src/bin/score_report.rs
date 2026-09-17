//! 评分解释层 bin：复刻并增强 URA（UmamusumeResponseAnalyzer）的评分预测功能。
//!
//! # 名词先用大白话解释
//!
//! - **URA**：社区工具「UmamusumeResponseAnalyzer」，育成结束时能给出
//!   「技能推荐表 + 评分摘要 + US 等级」。本 bin 在模拟器里复刻这套输出，
//!   并加了 URA 没有的东西（拉面杯结算口径、买入验证、跨卡组对比）。
//! - **两种口径**：URA 口径（技能点花掉才算分）和本仓库拉面杯口径
//!   （技能点每 1 点直接值 2 分，买技能多数是亏的）。详见 `score_explain` 模块头注释。
//!
//! # 用法（Release）
//!
//! ```text
//! cargo run --release --bin score_report -- [--genome-file best_genome.toml]
//!     [--deck id1,id2,id3,id4,id5[,friend]] [--uma GAMEID] [--friend IDRANK]
//!     [--seed S] [--runs N] [--pool all|deck-hint] [--top N] [--verify]
//!     [--builds-all] [--json] [--out FILE]
//! ```
//!
//! 缺省行为：读 workspace 根 bench_config.toml 第一个 build，用正式手写策略跑
//! 1 局，然后输出：
//! 1. 技能推荐表（URA 口径 + 本仓库口径各一张，按各自性价比降序）
//! 2. 评分摘要（两口径：五维评分、技能点三项、已学/即将学技能分、属性分、
//!    预测总分、US 等级、距下一档差分）
//! 3. 平均/边际性价比
//! 4. 公式对照（URA vs 本仓库）
//!
//! 增强项（URA 没做的）：
//! - `--verify`：把推荐技能实际买入后重算结算分，输出「预测 Δ分 vs 实际 Δ分」
//!   偏差表（应逐位为 0，非 0 说明计分管道有 bug）。
//! - `--builds-all`：bench_config.toml 全部 player_builds 各跑 1 局，
//!   输出同一技能在不同卡组下的性价比对比表（deck-hint 口径下不同卡组的
//!   可买集合与折扣不同，表有真实差异）。
//! - `--json`：输出 JSON，便于上游 URA 插件等程序消费。
//!
//! # 设计边界（上游同步安全）
//!
//! 本 bin 只读终局状态，不改模拟器逻辑；不传参数不影响任何现有 bin。

use anyhow::{Context, Result};
use lexopt::Arg;
use serde::Deserialize;
use umasim::bench::{self, CardPickOpts, load_player_builds};
use umasim::game::InheritInfo;
use umasim::gamedata::{GAMEDATA, RamenRegionStrategy, init_global_with_config};
use umasim::global;
use umasim::score_explain::{
    BuyPolicy, LocalScoring, SkillDb, SkillPool, UmaSnapshot, formula_compare, print_summary,
    recommend, summary_local, summary_ura, verify_delta
};
use umasim::trainer::RecommendedRamenTrainer;
use umasim::utils::{get_workspace_root, load_game_config};

/// CLI 配置（bench_config.toml 提供缺省的 uma/friend/蓝因子）
#[derive(Debug, Clone)]
struct ReportConfig {
    /// 马娘 id
    uma: u32,
    /// 固定友人卡 idrank
    friend: u32,
    /// 种马蓝因子个数
    blue_count: [i32; 5],
    /// 种马额外属性
    extra_count: [i32; 6],
    /// 局数（--verify 模式下每局都验证）
    runs: usize,
    /// 基础种子
    seed: u64,
    /// 技能池口径
    pool: SkillPool,
    /// 推荐表展示条数
    top: usize,
    /// 买入验证模式
    verify: bool,
    /// 跨卡组对比模式（全部 preset builds）
    builds_all: bool,
    /// JSON 输出
    json: bool,
    /// genome 覆盖层（GA 最优轮回放）
    genome_file: Option<String>,
    /// 自定义卡组覆盖
    deck: Option<String>
}

impl Default for ReportConfig {
    fn default() -> Self {
        Self {
            uma: 102601,
            friend: 303054,
            blue_count: [12, 0, 0, 0, 6],
            extra_count: [10, 0, 0, 20, 20, 40],
            runs: 1,
            seed: 42,
            pool: SkillPool::All,
            top: 15,
            verify: false,
            builds_all: false,
            json: false,
            genome_file: None,
            deck: None
        }
    }
}

/// bench_config.toml 里只提取本 bin 关心的字段（其余忽略）
#[derive(Debug, Deserialize)]
struct BenchConfigLite {
    #[serde(default)]
    uma: Option<u32>,
    #[serde(default)]
    friend: Option<u32>,
    #[serde(default)]
    blue_count: Option<[i32; 5]>,
    #[serde(default)]
    extra_count: Option<[i32; 6]>,
    #[serde(default)]
    seed: Option<u64>
}

/// 解析 CLI 参数
fn apply_cli(mut cfg: ReportConfig) -> Result<ReportConfig> {
    let mut parser = lexopt::Parser::from_env();
    while let Some(arg) = parser.next()? {
        match arg {
            Arg::Long("uma") => cfg.uma = bench::parse_value(&mut parser, "uma")?,
            Arg::Long("friend") => cfg.friend = bench::parse_value(&mut parser, "friend")?,
            Arg::Long("seed") => cfg.seed = bench::parse_value(&mut parser, "seed")?,
            Arg::Long("runs") => cfg.runs = bench::parse_value(&mut parser, "runs")?,
            Arg::Long("top") => cfg.top = bench::parse_value(&mut parser, "top")?,
            Arg::Long("pool") => {
                let s: String = bench::parse_value(&mut parser, "pool")?;
                cfg.pool = SkillPool::parse(&s)?;
            }
            Arg::Long("verify") => cfg.verify = true,
            Arg::Long("builds-all") => cfg.builds_all = true,
            Arg::Long("json") => cfg.json = true,
            Arg::Long("genome-file") => {
                cfg.genome_file = Some(bench::parse_value(&mut parser, "genome-file")?)
            }
            Arg::Long("deck") => cfg.deck = Some(bench::parse_value(&mut parser, "deck")?),
            Arg::Long("help") | Arg::Short('h') => {
                println!(
                    "用法: score_report [--genome-file best_genome.toml] [--deck id1,id2,id3,id4,id5[,friend]]
                      [--uma GAMEID] [--friend IDRANK] [--seed S] [--runs N]
                      [--pool all|deck-hint]（技能池：all=全池原价，deck-hint=卡组hint技能带折扣）
                      [--top N]（推荐表条数，缺省 15）
                      [--verify]（预测Δ分 vs 实际Δ分 偏差表）
                      [--builds-all]（全部 preset builds 跨卡组性价比对比）
                      [--json]（JSON 输出）"
                );
                std::process::exit(0);
            }
            other => anyhow::bail!("未知参数: {other:?}（可用 --help 查看用法）")
        }
    }
    Ok(cfg)
}

/// 读取 bench_config.toml 里的缺省参数（文件缺失/字段缺失时保持内置默认）
fn apply_bench_config(workspace_root: &std::path::Path, cfg: &mut ReportConfig) {
    let path = workspace_root.join("bench_config.toml");
    let Ok(text) = std::fs::read_to_string(&path) else {
        println!("提示: 未找到 bench_config.toml，使用内置默认参数");
        return;
    };
    let Ok(lite) = toml::from_str::<BenchConfigLite>(&text) else {
        println!("提示: bench_config.toml 解析失败，使用内置默认参数");
        return;
    };
    if let Some(v) = lite.uma {
        cfg.uma = v;
    }
    if let Some(v) = lite.friend {
        cfg.friend = v;
    }
    if let Some(v) = lite.blue_count {
        cfg.blue_count = v;
    }
    if let Some(v) = lite.extra_count {
        cfg.extra_count = v;
    }
    if let Some(v) = lite.seed {
        cfg.seed = v;
    }
}

/// 解析 `--deck` 覆盖串（与 bench_base 同规则：5 个 idrank 或 6 个含友人）
fn parse_deck_override(s: &str, friend: u32) -> Result<[u32; 6]> {
    let parts: Vec<&str> = s
        .split(',')
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .collect();
    anyhow::ensure!(
        parts.len() == 5 || parts.len() == 6,
        "--deck 需要 5 个支援卡 idrank（友人可省略）或 6 个含友人，收到 {} 个: {s}",
        parts.len()
    );
    let v = parts
        .iter()
        .map(|t| t.parse::<u32>())
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| anyhow::anyhow!("--deck idrank 解析失败: {e}"))?;
    let mut deck = [0u32; 6];
    deck[..5].copy_from_slice(&v[..5]);
    deck[5] = if v.len() == 6 { v[5] } else { friend };
    Ok(deck)
}

/// 跑一局并抄终局快照（复用 bench::seeded_rngs 的种子派生，与 bench_base 同口径）
fn run_one(
    uma_id: u32,
    deck: &[u32; 6],
    inherit: &InheritInfo,
    base_seed: u64,
    run_idx: u64,
    trainer: &RecommendedRamenTrainer
) -> Result<UmaSnapshot> {
    use umasim::game::{Game, ramen::RamenGame};
    let (mut decision_rng, rule_master) = bench::seeded_rngs(base_seed, run_idx);
    let mut game = RamenGame::newgame(uma_id, deck, inherit.clone())?;
    game.set_rule_master(rule_master);
    game.run_full_game(trainer, &mut decision_rng)?;
    let snap = UmaSnapshot::from_uma(&game.uma);
    Ok(snap)
}

fn main() -> Result<()> {
    // 切换到 workspace 根（bench_config.toml / gamedata 相对路径依赖）
    let workspace_root = get_workspace_root()?;
    std::env::set_current_dir(&workspace_root)?;

    let mut cfg = apply_cli(ReportConfig::default())?;
    apply_bench_config(&workspace_root, &mut cfg);

    // 初始化全局数据（与 bench_base 同约定；Y3 地区选择交回策略）
    let mut game_config = load_game_config()?;
    game_config.ramen_region_strategy = RamenRegionStrategy::All;
    game_config.ramen_region_fixed = None;
    init_global_with_config(&game_config)?;
    rayon::ThreadPoolBuilder::new()
        .num_threads(game_config.collector.threads)
        .build_global()?;

    let db = SkillDb::load()?;
    let local = LocalScoring::from_constants();
    let data = global!(GAMEDATA);
    let uma_name = data.get_uma(cfg.uma)?.name.clone();
    let inherit = InheritInfo {
        blue_count: cfg.blue_count,
        extra_count: cfg.extra_count
    };

    // 卡组：--deck 覆盖 > preset builds（--builds-all 用全部，否则用第一个）
    let pick = CardPickOpts::default();
    let builds = load_player_builds()?;
    anyhow::ensure!(!builds.is_empty(), "bench_config.toml 没有任何 player_builds");
    let deck_jobs: Vec<(String, [u32; 6])> = if let Some(ds) = &cfg.deck {
        vec![("custom_deck".to_string(), parse_deck_override(ds, cfg.friend)?)]
    } else if cfg.builds_all {
        builds
            .iter()
            .map(|b| Ok((b.name(), b.make_deck(&pick, cfg.friend)?)))
            .collect::<Result<Vec<_>>>()?
    } else {
        let b = &builds[0];
        vec![(b.name(), b.make_deck(&pick, cfg.friend)?)]
    };

    // 训练员：genome 覆盖层 > 正式 preset
    let trainer = if let Some(gf) = &cfg.genome_file {
        let toml_str = std::fs::read_to_string(gf)
            .with_context(|| format!("无法读取 --genome-file {gf}"))?;
        let ov = umasim::trainer::local_ramen_trainer::parse_override_toml(&toml_str)
            .map_err(|e| anyhow::anyhow!("--genome-file 解析失败: {e}"))?;
        println!("已加载基因组覆盖层: {gf}");
        RecommendedRamenTrainer::with_overrides(&ov)
    } else {
        RecommendedRamenTrainer::new()
    };

    println!(
        "===== score_report: uma={} {} seed={} runs={} pool={:?} top={} =====",
        cfg.uma, uma_name, cfg.seed, cfg.runs, cfg.pool, cfg.top
    );
    for (name, deck) in &deck_jobs {
        let cards_desc = deck
            .iter()
            .map(|id| match data.get_card(id / 10) {
                Ok(card) => format!("{} {}", id, card.card_name),
                Err(_) => id.to_string()
            })
            .collect::<Vec<_>>()
            .join(", ");
        println!("[build] {name} 卡组: [{cards_desc}]");
    }

    // ===== 主流程：每局跑一次，逐局产出 =====
    let mut json_out = serde_json::Map::new();
    let mut verify_rows: Vec<(usize, String, i32, i32, i32, i32)> = Vec::new();
    for (build_idx, (build_name, deck)) in deck_jobs.iter().enumerate() {
        let _ = build_idx; // 保留索引用于将来按 build 区分输出
        let mut last_snap: Option<UmaSnapshot> = None;
        for i in 0..cfg.runs {
            let snap = run_one(cfg.uma, deck, &inherit, cfg.seed, i as u64, &trainer)?;
            let score = local.base_score(&snap);
            let rank = umasim::gamedata::GAMECONSTANTS
                .get()
                .expect("GAMECONSTANTS")
                .get_rank_name(score);
            println!(
                "  [#{:02}] {} seed={} 结算分={} ({}) 五维={:?} skill_pt={} hints={}",
                i + 1,
                build_name,
                cfg.seed + i as u64,
                score,
                rank,
                snap.five_status,
                snap.skill_pt,
                snap.total_hints
            );

            // 验证模式：top-N 推荐技能实际买入后重算，预测 Δ vs 实际 Δ
            if cfg.verify && !cfg.builds_all {
                let plan = recommend(&db, &snap, cfg.pool, deck, cfg.top, BuyPolicy::UraValue);
                let (pred, act, dev) = verify_delta(&local, &snap, &plan);
                println!(
                    "    验证: 买入{}个技能 花费{}PT 预测Δ={} 实际Δ={} 偏差={}",
                    plan.buys.len(),
                    plan.cost_total,
                    pred,
                    act,
                    dev
                );
                verify_rows.push((i + 1, build_name.clone(), score, pred, act, dev));
            }
            last_snap = Some(snap);
        }

        // ===== 完整摘要（只用最后一局状态；跨卡组模式每个 build 都出推荐表）=====
        let snap = last_snap.expect("至少跑了 1 局");
        let plan_ura = recommend(&db, &snap, cfg.pool, deck, cfg.top, BuyPolicy::UraValue);
        let plan_local = recommend(&db, &snap, cfg.pool, deck, cfg.top, BuyPolicy::LocalDelta);
        let sum_ura = summary_ura(&db, &snap, &plan_ura);
        let sum_local = summary_local(&local, &snap, &plan_local);

        if !cfg.json {
            println!();
            println!("===== 技能推荐表（URA口径，性价比=评分增量/价格 降序，{}）=====", build_name);
            println!("{:<4} {:<24} {:>6} {:>8} {:>10} {:>8}", "#", "技能名", "技能点", "评价点", "性价比", "净增分*");
            for (i, c) in plan_ura.buys.iter().take(cfg.top).enumerate() {
                println!(
                    "{:<4} {:<24} {:>6} {:>8} {:>10.3} {:>8}",
                    i + 1,
                    c.name,
                    c.price,
                    c.grade,
                    c.value_ura,
                    c.delta_local
                );
            }
            let avg_value = if plan_ura.cost_total > 0 {
                plan_ura.grade_total as f64 / plan_ura.cost_total as f64
            } else {
                0.0
            };
            println!(
                "平均性价比（总评价点/总技能点）: {:.3}；总评价点={} 总花费={}",
                avg_value, plan_ura.grade_total, plan_ura.cost_total
            );
            println!("* 净增分 = 本仓库拉面杯口径的 grade−价格×2（URA 口径下无意义，见下表）");

            println!();
            println!("===== 技能推荐表（本仓库拉面杯口径，净增分=评分增量−价格×2 降序）=====");
            println!("{:<4} {:<24} {:>6} {:>8} {:>10} {:>10}", "#", "技能名", "技能点", "评价点", "性价比", "净增分");
            for (i, c) in plan_local.buys.iter().take(cfg.top).enumerate() {
                println!(
                    "{:<4} {:<24} {:>6} {:>8} {:>10.3} {:>10}",
                    i + 1,
                    c.name,
                    c.price,
                    c.grade,
                    c.value_ura,
                    c.delta_local
                );
            }
            println!(
                "口径要点：拉面杯结算里技能点本身值 2 分/点，买技能是『花 2 分/点的本钱换评价点』；\n\
                 净增分为负的技能不值得买。本轮符合条件的有 {} 个，合计净增 {} 分。",
                plan_local.buys.len(),
                plan_local.delta_local_total
            );

            println!();
            print_summary(&sum_ura);
            println!();
            print_summary(&sum_local);
            println!();
            println!("---- 口径对照 ----");
            for line in formula_compare(&local, &snap, &db) {
                println!("  - {line}");
            }
        }
    }

    // ===== 跨卡组性价比对比（--builds-all；deck-hint 口径下才有真实差异）=====
    if cfg.builds_all {
        println!();
        println!("===== 跨卡组性价比稳定性（{}口径，同一技能 × 各 build 终局）=====", match cfg.pool { SkillPool::All => "all", SkillPool::DeckHint => "deck-hint" });
        // 收集每个 build 的 top 技能性价比
        let mut per_build: Vec<(String, Vec<(String, f64, i32)>)> = Vec::new();
        for (build_name, deck) in &deck_jobs {
            let snap = run_one(cfg.uma, deck, &inherit, cfg.seed, 0, &trainer)?;
            let mut cands: Vec<(String, f64, i32)> = db
                .buyable(cfg.pool, deck)
                .into_iter()
                .map(|(s, price)| {
                    let v = if price > 0 {
                        s.grade as f64 / price as f64
                    } else {
                        0.0
                    };
                    (s.display_name(), v, price)
                })
                .collect();
            cands.sort_by(|a, b| b.1.total_cmp(&a.1));
            per_build.push((build_name.clone(), cands.into_iter().take(cfg.top).collect()));
            println!("  [{}] 终局结算分={}", build_name, local.base_score(&snap));
        }
        // 行=技能（各 build top 并集），列=build 性价比
        let mut all_names: Vec<String> = Vec::new();
        for (_, cands) in &per_build {
            for (n, _, _) in cands {
                if !all_names.iter().any(|x| x == n) {
                    all_names.push(n.clone());
                }
            }
        }
        println!("{:<24}", "技能名");
        for (name, _) in &per_build {
            print!(" {:>12}", name);
        }
        println!("   跨build差异");
        for name in &all_names {
            print!("{:<24}", name);
            let mut vals = Vec::new();
            for (_, cands) in &per_build {
                match cands.iter().find(|(n, _, _)| n == name) {
                    Some((_, v, _)) => {
                        print!(" {:>12.3}", v);
                        vals.push(*v);
                    }
                    None => print!(" {:>12}", "-")
                }
            }
            let diff = if vals.len() >= 2 {
                let mn = vals.iter().cloned().fold(f64::INFINITY, f64::min);
                let mx = vals.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
                format!("min={mn:.3} max={mx:.3}")
            } else {
                "仅1个build".to_string()
            };
            println!("   {diff}");
        }
        println!("说明: all 口径下技能价格与评分不随卡组变化，性价比恒同（差异=0 是预期）；\n\
                  deck-hint 口径下不同卡组的可买集合与折扣不同，表有真实差异。");
    }

    // ===== JSON 输出 =====
    if cfg.json {
        let out_path = workspace_root.join("logs/score_report.json");
        std::fs::create_dir_all(out_path.parent().unwrap())?;
        let mut builds_json = Vec::new();
        for (build_name, deck) in &deck_jobs {
            let snap = run_one(cfg.uma, deck, &inherit, cfg.seed, 0, &trainer)?;
            let plan_ura = recommend(&db, &snap, cfg.pool, deck, cfg.top, BuyPolicy::UraValue);
            let plan_local = recommend(&db, &snap, cfg.pool, deck, cfg.top, BuyPolicy::LocalDelta);
            builds_json.push(serde_json::json!({
                "build": build_name,
                "deck_idrank": deck,
                "base_score_local": local.base_score(&snap),
                "snapshot": {
                    "five_status": snap.five_status,
                    "skill_pt": snap.skill_pt,
                    "total_hints": snap.total_hints,
                    "skill_score": snap.skill_score,
                    "total_pt": snap.total_pt()
                },
                "summary_ura": summary_ura(&db, &snap, &plan_ura),
                "summary_local": summary_local(&local, &snap, &plan_local),
                "plan_ura": plan_ura,
                "plan_local": plan_local,
                "formula_compare": formula_compare(&local, &snap, &db)
            }));
        }
        json_out.insert("uma".into(), serde_json::json!(cfg.uma));
        json_out.insert("seed".into(), serde_json::json!(cfg.seed));
        json_out.insert("pool".into(), serde_json::json!(match cfg.pool { SkillPool::All => "all", SkillPool::DeckHint => "deck-hint" }));
        json_out.insert("builds".into(), serde_json::Value::Array(builds_json));
        std::fs::write(&out_path, serde_json::to_string_pretty(&json_out)?)?;
        println!("\nJSON 已写入: {}", out_path.display());
    }

    // ===== 验证偏差汇总 =====
    if cfg.verify && !cfg.builds_all {
        println!();
        println!("===== 验证偏差表（预测Δ vs 实际Δ，应逐位为 0）=====");
        println!("{:<6} {:<14} {:>10} {:>10} {:>10} {:>8}", "局", "build", "基础分", "预测Δ", "实际Δ", "偏差");
        let mut max_dev = 0i32;
        for (n, b, base, pred, act, dev) in &verify_rows {
            if pred == &0 && act == &0 {
                continue; // 非验证行跳过
            }
            println!("{:<6} {:<14} {:>10} {:>10} {:>10} {:>8}", n, b, base, pred, act, dev);
            max_dev = max_dev.max(dev.abs());
        }
        println!("最大绝对偏差: {max_dev}（0 = 计分管道与解析式完全一致）");
    }

    Ok(())
}
