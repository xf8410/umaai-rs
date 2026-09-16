//! GA 参数优化 bin：按 design.md 定稿协议跑遗传算法搜索拉面策略参数。
//!
//! 与 `bench_base` 同源同序的引导（workspace 根、全局数据注入、地区策略
//! 交回策略层、线程池），评估 100% 复用 [`umasim::bench`] 的固定种子跑批，
//! 保证 GA 个体与基线在同一把种子下可比。
//!
//! # 用法（Release）
//!
//! ```text
//! cargo run --release --bin ga_optimize -- [--pop N] [--gens N] [--out DIR] ...
//! ```
//!
//! 缺省参数 = design.md 定稿值（种群 60 / 40 代 / 精英 4 / 锦标赛 3 /
//! 交叉 0.9 / σ 0.15→0.03 / 停滞 5 代 / 初筛 3build×20 局 / 精评 7build×60 局 /
//! holdout 7build×40 局 / base_seed 42 / holdout_seed 43 / λ_race 800）。
//! 马娘/友人/继承因子读取 workspace 根 `bench_config.toml`（与基线同源）。
//!
//! # 产出（默认 `ga_logs/`）
//!
//! - `ga_generations.csv`：逐代摘要（最优/均值适应度、精评个体数、缓存命中、σ、停滞计数、holdout）
//! - `ga_detail.csv`：逐局明细（评估级别 + 基因组哈希 + 适应度 + bench 31 列标准行）
//! - `best_genome.toml`：最优基因组的覆盖层 Some 子集（preset 快照，可直接人工评审）
//! - `preset_baseline.toml`：基因表 preset 锚点快照（None 表示该位 preset 不可单值表示）
//!
//! # 注意
//!
//! 本 bin 只负责把 GA 搜索跑起来并落盘；**不要**在未确认预算的情况下直接跑
//! 全量协议（默认参数每代约 5280 局，40 代约 21 万局）。

use anyhow::{Context, Result};
use lexopt::Arg;
use rayon::ThreadPoolBuilder;
use serde::Deserialize;
use umasim::card_pool::SsrPool;
use umasim::genetic_optimizer::{
    GENE_COUNT, GENE_SPECS, GaGenome, GaOptimizer, GaParams, FitnessEvaluator,
    SimFitnessEvaluator, select_screen_builds
};
use umasim::{
    bench::{self, RESULTS_HEADER, load_player_builds},
    game::InheritInfo,
    gamedata::{GAMEDATA, RamenRegionStrategy, init_global_with_config},
    global,
    trainer::ParamOverride,
    utils::{get_workspace_root, load_game_config},
};

/// bench_config.toml 中 GA 需要的项（马娘/友人/继承因子；其余忽略）
/// 内置默认值见各字段文档（与 bench_config.toml 一致；文件缺失/字段缺失时使用）
#[derive(Debug, Clone, Deserialize)]
#[serde(default = "default_ga_bench_config")]
struct GaBenchConfig {
    /// 马娘 ID
    uma: u32,
    /// 固定友人卡 idrank（build 卡组生成用）
    friend: u32,
    /// 种马蓝因子个数
    blue_count: [i32; 5],
    /// 种马额外属性（内置默认 [10, 10, 20, 20, 20, 40]）
    extra_count: [i32; 6]
}

/// 内置默认（与 bench_config.toml 一致；文件缺失或字段缺失时使用）
fn default_ga_bench_config() -> GaBenchConfig {
    GaBenchConfig {
        uma: 102601,
        friend: 303054,
        blue_count: [15, 0, 0, 0, 3],
        extra_count: [10, 10, 20, 20, 20, 40]
    }
}


/// 读取 bench_config.toml 的 uma/friend/继承因子段；缺失时用内置默认
fn load_ga_bench_config(workspace_root: &std::path::Path) -> Result<GaBenchConfig> {
    let path = workspace_root.join("bench_config.toml");
    if path.exists() {
        let text = std::fs::read_to_string(&path)
            .with_context(|| format!("读取 bench_config.toml 失败: {}", path.display()))?;
        // 只取 GA 需要的键；未知字段（runs/seed/trainer...）由 serde 默认忽略
        let cfg: GaBenchConfig = toml::from_str(&text)
            .with_context(|| format!("解析 bench_config.toml 失败: {}", path.display()))?;
        return Ok(cfg);
    }
    println!("提示: 未找到 bench_config.toml，使用内置默认马娘/友人/继承因子");
    Ok(default_ga_bench_config())
}

/// 从 gamedata/umaDB.json 随机抽一个育成马娘 gameId。
///
/// 用系统时间纳秒做种子——每次 CI dispatch 抽到不同马娘，配合
/// workflow 的 random 模式实现"每个马都随机跑"。本体卡剔除由
/// 现有 --uma 通道自动处理（gameId/100 换算 chara_id）。
fn random_uma_from_db(workspace_root: &std::path::Path) -> Result<u32> {
    let path = workspace_root.join("gamedata/umaDB.json");
    let text = std::fs::read_to_string(&path)
        .with_context(|| format!("读取 umaDB 失败: {}", path.display()))?;
    let db: serde_json::Map<String, serde_json::Value> = serde_json::from_str(&text)
        .with_context(|| format!("解析 umaDB 失败: {}", path.display()))?;
    let entries: Vec<(u32, String)> = db
        .values()
        .filter_map(|v| {
            let gid = v.get("gameId")?.as_u64()? as u32;
            let name = v.get("name").and_then(|n| n.as_str()).unwrap_or("?").to_string();
            Some((gid, name))
        })
        .collect();
    anyhow::ensure!(!entries.is_empty(), "umaDB 无有效马娘");
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .subsec_nanos() as usize;
    let (gid, name) = &entries[nanos % entries.len()];
    println!("random-uma: {} {}（池 {} 骑）", gid, name, entries.len());
    Ok(*gid)
}

/// CLI 解析：GA 协议参数全部可调（缺省 = design.md 定稿值）
/// 返回 (GaParams, GaBenchConfig, out_dir, exclude_chara_ids)
fn apply_cli(mut params: GaParams, mut cfg: GaBenchConfig, mut out_dir: String) -> Result<(GaParams, GaBenchConfig, String, Vec<u32>)> {
    let mut exclude_charas: Vec<u32> = Vec::new();
    let mut parser = lexopt::Parser::from_env();
    while let Some(arg) = parser.next()? {
        match arg {
            Arg::Long("pop") => params.pop = bench::parse_value(&mut parser, "pop")?,
            Arg::Long("gens") => params.gens = bench::parse_value(&mut parser, "gens")?,
            Arg::Long("elitism") => params.elitism = bench::parse_value(&mut parser, "elitism")?,
            Arg::Long("k") => params.tournament_k = bench::parse_value(&mut parser, "k")?,
            Arg::Long("crossover") => {
                params.crossover_rate = bench::parse_value(&mut parser, "crossover")?
            }
            Arg::Long("sigma0") => params.sigma0 = bench::parse_value(&mut parser, "sigma0")?,
            Arg::Long("sigma-min") => {
                params.sigma_min = bench::parse_value(&mut parser, "sigma-min")?
            }
            Arg::Long("stagnation") => {
                params.stagnation_gens = bench::parse_value(&mut parser, "stagnation")?
            }
            Arg::Long("seed") => params.ga_seed = bench::parse_value(&mut parser, "seed")?,
            Arg::Long("base-seed") => {
                params.base_seed = bench::parse_value(&mut parser, "base-seed")?
            }
            Arg::Long("holdout-seed") => {
                params.holdout_seed = bench::parse_value(&mut parser, "holdout-seed")?
            }
            Arg::Long("screen-builds") => {
                params.screen_builds = bench::parse_value(&mut parser, "screen-builds")?
            }
            Arg::Long("screen-runs") => {
                params.screen_runs = bench::parse_value(&mut parser, "screen-runs")?
            }
            Arg::Long("full-builds") => {
                params.full_builds = bench::parse_value(&mut parser, "full-builds")?
            }
            Arg::Long("full-runs") => {
                params.full_runs = bench::parse_value(&mut parser, "full-runs")?
            }
            Arg::Long("holdout-runs") => {
                params.holdout_runs = bench::parse_value(&mut parser, "holdout-runs")?
            }
            Arg::Long("lambda-race") => {
                params.lambda_race = bench::parse_value(&mut parser, "lambda-race")?
            }
            Arg::Long("none-ratio") => {
                params.none_gene_ratio = bench::parse_value(&mut parser, "none-ratio")?
            }
            Arg::Long("uma") => cfg.uma = bench::parse_value(&mut parser, "uma")?,
            Arg::Long("random-uma") => {
                cfg.uma = random_uma_from_db(&get_workspace_root()?)?;
            }
            Arg::Long("friend") => cfg.friend = bench::parse_value(&mut parser, "friend")?,
            Arg::Long("out") => out_dir = bench::parse_value(&mut parser, "out")?,
            Arg::Long("exclude-chara") => {
                let cid: u32 = bench::parse_value(&mut parser, "exclude-chara")?;
                exclude_charas.push(cid);
            }
            Arg::Long("help") | Arg::Short('h') => {
                println!(
                    "用法: ga_optimize [--pop N] [--gens N] [--elitism N] [--k N] [--crossover F]\n\
                     \x20                 [--sigma0 F] [--sigma-min F] [--stagnation N] [--seed S]\n\
                     \x20                 [--base-seed S] [--holdout-seed S]\n\
                     \x20                 [--screen-builds N] [--screen-runs N] [--full-builds N]\n\
                     \x20                 [--full-runs N] [--holdout-runs N]\n\
                     \x20                 [--lambda-race F] [--none-ratio F]\n\
                     \x20                 [--uma ID] [--friend ID] [--out DIR]\n\
                     \x20                 [--exclude-chara ID] ...（可传多次）\n\
                     缺省 = design.md 定稿协议；马娘/友人/继承因子读 bench_config.toml\n\
                     本体卡自动剔除：--uma 的 chara_id（gameId/100）自动加入排除\n\
                     ⚠️ 全量协议每代约 5280 局、40 代约 21 万局，跑前确认预算"
                );
                std::process::exit(0);
            }
            other => {
                anyhow::bail!("未知参数: {other:?}（可用 --help 查看用法）");
            }
        }
    }
    Ok((params, cfg, out_dir, exclude_charas))
}

/// 基因表 preset 锚点快照 → TOML 文本（None = 该位 preset 不可单值表示）
fn preset_baseline_toml() -> String {
    let mut s = String::from(
        "# GA 基因表 preset 锚点快照（自动生成）\n\
         # 来源：RecommendedRamenTrainer::new() 正式 preset\n\
         # 值为 none 表示该参数的 preset 状态不可用单一数值表示（覆盖层保持 None 即保留 preset）\n\n"
    );
    for (layer_name, layer) in [("policy", umasim::genetic_optimizer::GeneLayer::Policy), ("local", umasim::genetic_optimizer::GeneLayer::Local)] {
        s.push_str(&format!("[{layer_name}]\n"));
        for spec in GENE_SPECS.iter().filter(|s| s.layer == layer) {
            match spec.preset {
                Some(p) => {
                    if spec.kind == umasim::genetic_optimizer::GeneKind::Int {
                        s.push_str(&format!("{} = {}\n", spec.name, p as i64));
                    } else if spec.kind == umasim::genetic_optimizer::GeneKind::Bool {
                        s.push_str(&format!("{} = {}\n", spec.name, p >= 0.5));
                    } else {
                        s.push_str(&format!("{} = {}\n", spec.name, p));
                    }
                }
                None => s.push_str(&format!("{} = \"none\"\n", spec.name))
            }
        }
        s.push('\n');
    }
    s
}

/// 最优覆盖层的 Some 子集 → TOML 文本（[policy] / [local] 分节，只写 Some 字段）。
///
/// 分节依据 GENE_SPECS 的 layer 元数据；手工维护字段名与覆盖层定义一致，
/// 编译器由 `if let Some` 保证类型正确。
fn override_to_toml(ov: &ParamOverride) -> String {
    let mut policy = String::from("[policy]\n");
    let mut local = String::from("[local]\n");
    macro_rules! p_f32 {
        ($field:ident) => {
            if let Some(v) = ov.$field {
                policy.push_str(&format!("{} = {}\n", stringify!($field), v));
            }
        };
    }
    macro_rules! p_i32 {
        ($field:ident) => {
            if let Some(v) = ov.$field {
                policy.push_str(&format!("{} = {}\n", stringify!($field), v));
            }
        };
    }
    macro_rules! p_bool {
        ($field:ident) => {
            if let Some(v) = ov.$field {
                policy.push_str(&format!("{} = {}\n", stringify!($field), v));
            }
        };
    }
    macro_rules! l_f32 {
        ($field:ident) => {
            if let Some(v) = ov.$field {
                local.push_str(&format!("{} = {}\n", stringify!($field), v));
            }
        };
    }
    macro_rules! l_i32 {
        ($field:ident) => {
            if let Some(v) = ov.$field {
                local.push_str(&format!("{} = {}\n", stringify!($field), v));
            }
        };
    }
    macro_rules! l_bool {
        ($field:ident) => {
            if let Some(v) = ov.$field {
                local.push_str(&format!("{} = {}\n", stringify!($field), v));
            }
        };
    }

    // ---- policy 层 ----
    p_i32!(vital_rest);
    if ov.vital_rest_eating.iter().any(|v| v.is_some()) {
        let vals: Vec<String> = ov
            .vital_rest_eating
            .iter()
            .map(|v| v.map(|x| x.to_string()).unwrap_or_else(|| "null".to_string()))
            .collect();
        policy.push_str(&format!("vital_rest_eating = [{}]\n", vals.join(", ")));
    }
    p_i32!(wisdom_vital_floor);
    p_i32!(motivation_outing);
    p_f32!(status_rate);
    if ov.pt_rate.iter().any(|v| v.is_some()) {
        let vals: Vec<String> = ov
            .pt_rate
            .iter()
            .map(|v| v.map(|x| x.to_string()).unwrap_or_else(|| "null".to_string()))
            .collect();
        policy.push_str(&format!("pt_rate = [{}]\n", vals.join(", ")));
    }
    p_f32!(pt_tradeoff);
    p_f32!(pt_tradeoff_shining);
    p_f32!(pt_tradeoff_super);
    p_f32!(cap_discount_weight);
    p_f32!(failure_penalty);
    p_bool!(effective_ramen_failure);
    p_f32!(shining_bonus);
    p_f32!(train_vital_value);
    p_f32!(rest_base);
    p_f32!(rest_vital_value);
    p_i32!(rest_target_vital);
    p_f32!(race_panel_discount);
    p_f32!(race_free_urgency_weight);
    if let Some(v) = ov.race_gate_slack {
        policy.push_str(&format!("race_gate_slack = {}\n", v));
    }
    p_f32!(outing_base);
    p_f32!(friend_outing_bonus);
    p_f32!(ramen_pt_weight);
    p_f32!(ramen_effect_weight);
    p_f32!(ramen_special_cost);
    p_f32!(ramen_stock_cost);
    p_f32!(region_xunlian_weight);
    p_f32!(region_hint_weight);
    p_f32!(region_youqing_weight);
    p_f32!(region_weak_cover_weight);
    p_f32!(event_vital_weight);
    p_f32!(event_motivation_weight);
    p_f32!(event_bad_flag_penalty);
    // ---- local 层 ----
    l_f32!(early_bond_value);
    l_f32!(hint_bonus);
    l_f32!(first_friend_click_value);
    l_f32!(low_friend_bond_value);
    l_f32!(active_friend_value);
    l_i32!(feeling_overflow_threshold);
    l_f32!(overflow_value);
    l_f32!(max_base_score_sacrifice);
    l_f32!(status_reserve_max);
    l_bool!(dynamic_status_balance);
    l_f32!(status_gap_strength);
    l_f32!(status_overflow_strength);
    l_bool!(dynamic_vital);
    l_bool!(probabilistic_hint);
    l_bool!(expected_fail);
    l_f32!(checkpoint_scale);
    l_f32!(rmj_cross_bonus);
    l_f32!(great_cross_bonus);
    l_f32!(ramen_window_weight);
    l_f32!(ramen_train_coupling_weight);
    l_f32!(ramen_weak_train_boost);
    l_f32!(friend_hidden_starve_weight);
    l_f32!(friend_future_hidden_weight);
    l_f32!(friend_proactive_weight);
    l_f32!(eat_guarantee_weight);
    l_f32!(cook2_stock_weight);
    l_bool!(eat_requires_training);
    l_bool!(eat_requires_covered_train);
    l_i32!(y3_pre_train_vital_target);
    l_i32!(y3_post_train_vital_target);
    l_f32!(y3_vital_shortfall_weight);
    l_i32!(y3_post_train_hard_floor);
    l_bool!(y3_recovery_horizon);
    l_bool!(friend_outing_replaces_rest);
    l_i32!(friend_outing3_recovery_vital);
    if ov.friend_outing_cumulative_caps.iter().any(|v| v.is_some()) {
        let vals: Vec<String> = ov
            .friend_outing_cumulative_caps
            .iter()
            .map(|v| v.map(|x| x.to_string()).unwrap_or_else(|| "null".to_string()))
            .collect();
        local.push_str(&format!("friend_outing_cumulative_caps = [{}]\n", vals.join(", ")));
    }
    l_bool!(dynamic_special_targets);

    format!(
        "# GA 最优基因组覆盖层（Some 子集 preset 快照，自动生成）\n\
         # None 字段未列出 = 保留 RecommendedRamenTrainer::new() preset\n\
         # 适应度口径见 ga_generations.csv；本文件供人工评审与后续固定参数实验\n\n{policy}\n{local}"
    )
}

/// 打印 ScoreCard 的诊断摘要（适应度公式各分量 + 诊断列）
fn print_card_summary(label: &str, card: &umasim::genetic_optimizer::ScoreCard) {
    println!(
        "  [{label}] fitness={:.1} mean_score={:.1}±{:.1} skill_pt={:.1} scenario_pt={:?} race_fail={:.3} rmj_ok={:.2}/3 ({}build×{}runs)",
        card.fitness,
        card.mean_score,
        card.score_std,
        card.mean_skill_pt,
        [format!("{:.0}", card.mean_scenario_pt[0]), format!("{:.0}", card.mean_scenario_pt[1]), format!("{:.0}", card.mean_scenario_pt[2])],
        card.race_fail_rate,
        card.mean_rmj_ok,
        card.n_builds,
        card.runs_per_build
    );
}


fn main() -> Result<()> {
    // 切换到 workspace 根（bench_config.toml / gamedata 相对路径依赖）
    let workspace_root = get_workspace_root()?;
    std::env::set_current_dir(&workspace_root)?;

    let (params, cfg, out_dir_rel, mut exclude_charas) =
        apply_cli(GaParams::default(), load_ga_bench_config(&workspace_root)?, "ga_logs".to_string())?;

    // 自动剔除育成马娘本体卡：chara_id = gameId / 100
    let uma_chara_id = cfg.uma / 100;
    if !exclude_charas.contains(&uma_chara_id) {
        exclude_charas.push(uma_chara_id);
    }

    // 引导序列与 bench_base 完全一致（地区策略交回策略层 + 线程池）
    let mut game_config = load_game_config()?;
    game_config.ramen_region_strategy = RamenRegionStrategy::All;
    game_config.ramen_region_fixed = None;
    init_global_with_config(&game_config)?;
    ThreadPoolBuilder::new()
        .num_threads(game_config.collector.threads)
        .build_global()?;

    // 配卡说明：拉面杯友人卡由游戏机制限定为骏川手纲（FRIEND_IDRANK=30305 系，模拟器
    // 仅支持新友人卡组）；普通卡=GA 三维搜索（77 基因 × 101 布局 × 配卡选择）随机起步
    // + 逐代进化，随机马娘模式下每轮配卡路径天然不同。本体卡剔除由
    // SsrPool::load_filtered（chara_id = gameId/100）统一完成。
    let builds = load_player_builds()?;
    let uma_name = global!(GAMEDATA).get_uma(cfg.uma)?.name.clone();
    let inherit = InheritInfo {
        blue_count: cfg.blue_count,
        extra_count: cfg.extra_count
    };

    // 初筛代表 build（design §3.2：1 速主 / 1 耐主 / 1 智主，按声明序）
    let screen_names = select_screen_builds(&builds, params.screen_builds);
    let full_names: Vec<String> = builds.iter().map(|b| b.name.clone()).collect();

    let out_dir = workspace_root.join(&out_dir_rel);
    std::fs::create_dir_all(&out_dir)?;

    println!("===== ga_optimize: uma={} {} pop={} gens={} elitism={} k={} cx={:.2} σ {:.2}→{:.2} 停滞={} =====",
        cfg.uma, uma_name, params.pop, params.gens, params.elitism,
        params.tournament_k, params.crossover_rate, params.sigma0, params.sigma_min, params.stagnation_gens);
    println!("评估协议: 初筛 {}build×{}runs(seed={}) → 精评 {}build×{}runs(seed={}) | holdout {}runs(seed={})",
        params.screen_builds, params.screen_runs, params.base_seed,
        params.full_builds, params.full_runs, params.base_seed,
        params.holdout_runs, params.holdout_seed);
    println!("λ_race={} ga_seed={} none_ratio={:.2} 基因数={}",
        params.lambda_race, params.ga_seed, params.none_gene_ratio, GENE_COUNT);
    println!("builds({})={} 初筛代表={:?}", builds.len(), full_names.join(","), screen_names);
    {
        let budget_per_gen = params.screen_builds as u64 * params.screen_runs as u64 * params.pop as u64
            + params.full_builds as u64 * params.full_runs as u64 * params.elitism as u64
            + params.full_builds as u64 * params.holdout_runs as u64;
        println!("预算提示: 满额每代 ≈ {budget_per_gen} 局 × {} 代 ≈ {} 局（缓存命中会低于此值）",
            params.gens, budget_per_gen * params.gens as u64);
    }

    // preset 锚点快照（评审基线用，先落盘）
    std::fs::write(out_dir.join("preset_baseline.toml"), preset_baseline_toml())
        .context("写 preset_baseline.toml 失败")?;

    // 逐代摘要 CSV 表头
    let gen_header = [
        "generation",
        "best_fitness",
        "best_is_full",
        "mean_fitness",
        "full_count",
        "cache_hits",
        "sigma",
        "stagnation_counter",
        "holdout_fitness",
        "holdout_mean_score",
        "elapsed_ms"
    ];
    let detail_header: Vec<&str> = ["level", "genome_hash", "fitness"]
        .iter()
        .chain(RESULTS_HEADER.iter())
        .copied()
        .collect();
    bench::write_csv(&out_dir.join("ga_generations.csv"), &gen_header, &[])?;
    bench::write_csv(&out_dir.join("ga_detail.csv"), &detail_header, &[])?;

    // 构造评估器（卡组与 bench_base 同源同序：CardPickOpts::default() + make_deck）
    let pool = SsrPool::load_filtered(&exclude_charas).context("加载 SSR 卡池失败")?;
    println!("SSR 卡池: {} 张（排除 {} 张未实装卡；按 chara_id 剔除 {:?}）",
        pool.pools.iter().map(|p| p.len()).sum::<usize>(), pool.excluded.len(), pool.exclude_chara_ids);
    for (i, name) in umasim::card_pool::ATTR_NAMES.iter().enumerate() {
        println!("  {}{}: {} 张", name, umasim::card_pool::ATTR_NAMES_ZH[i], pool.pool_size(i));
    }
    // 卡名映射表（池序 card_id 降序，与 get_idrank 的 idx 对齐）：pool 交给 evaluator 前抽出，
    // 供最优卡组明细输出使用
    let pool_cards: Vec<Vec<(u32, String)>> = pool
        .pools
        .iter()
        .map(|cards| cards.iter().map(|c| (c.idrank, c.full_name.clone())).collect())
        .collect();
    let mut evaluator = SimFitnessEvaluator::new(cfg.uma, cfg.friend, inherit, params.clone(), pool)
        .context("构造 SimFitnessEvaluator 失败")?;

    let report = GaOptimizer::new(params.clone()).run(&mut evaluator)?;

    // 逐代摘要落盘
    let gen_rows: Vec<Vec<String>> = report
        .history
        .iter()
        .map(|h| {
            vec![
                h.generation.to_string(),
                format!("{:.4}", h.best_fitness),
                h.best_is_full.to_string(),
                format!("{:.4}", h.mean_fitness),
                h.full_count.to_string(),
                h.cache_hits.to_string(),
                format!("{:.4}", h.sigma),
                h.stagnation_counter.to_string(),
                h.holdout_fitness.map(|f| format!("{:.4}", f)).unwrap_or_default(),
                h.holdout_mean_score.map(|f| format!("{:.4}", f)).unwrap_or_default(),
                format!("{:.1}", h.elapsed_ms)
            ]
        })
        .collect();
    bench::write_csv(&out_dir.join("ga_generations.csv"), &gen_header, &gen_rows)?;

    // 明细落盘（GA 结束后一次性取走；单次运行规模可控）
    let details = evaluator.take_detail_rows();
    let detail_rows: Vec<Vec<String>> = details
        .into_iter()
        .map(|(level, key, fitness, row)| {
            let mut full = vec![
                level.tag().to_string(),
                format!("{key:016x}"),
                format!("{fitness:.4}")
            ];
            full.extend(row);
            full
        })
        .collect();
    bench::write_csv(&out_dir.join("ga_detail.csv"), &detail_header, &detail_rows)?;

    // 最优基因组快照
    std::fs::write(out_dir.join("best_genome.toml"), override_to_toml(&report.best_override))
        .context("写 best_genome.toml 失败")?;

    // 终端摘要
    println!("===== GA 完成 =====");
    println!("耗时 {:.0}ms | 评估计数 {:?}", report.elapsed_ms, report.counts);
    println!(
        "最优（精评）fitness={:.1} 基因组哈希 {:016x}",
        report.best_fitness,
        umasim::genetic_optimizer::genome_hash(&report.best_genome)
    );
    println!("最优配卡选择: {:?}", report.best_card_sel.indices);
    // 最优卡组完整明细：布局（每属性张数）+ 每张卡（idrank + 全名，连续段同属性相邻）
    {
        let counts = bench::all_compositions()[report.best_comp_idx];
        let mut desc = String::new();
        for (attr, &count) in counts.iter().enumerate() {
            if count == 0 {
                continue;
            }
            let names: Vec<String> = (0..count)
                .map(|j| {
                    let idx = report.best_card_sel.indices[attr] + j;
                    let (idrank, name) = &pool_cards[attr][idx];
                    format!("{idrank} {name}")
                })
                .collect();
            desc.push_str(&format!(
                " {}×{}[{}]",
                umasim::card_pool::ATTR_NAMES_ZH[attr],
                count,
                names.join(", ")
            ));
        }
        println!("最优卡组明细: comp_idx={} |{}", report.best_comp_idx, desc);
    }
    print_card_summary("精评", &report.best_card);
    if let Some(h) = report.holdout_card.as_ref() {
        print_card_summary("holdout", h);
    }
    if let Some(gap) = report.overfit_gap {
        println!("过拟合哨兵（精评 − holdout）= {gap:.1}（持续为正且增大即提示 seed 过拟合）");
    }
    println!("σ 受控重启次数 = {} | 逐代摘要见 {}", report.stagnation_resets, out_dir_rel);
    let best5 = report
        .history
        .iter()
        .rev()
        .take(5)
        .rev()
        .map(|h| format!("gen{}={:.0}", h.generation, h.best_fitness))
        .collect::<Vec<_>>()
        .join(" ");
    println!("末 5 代最优: {best5}");
    println!("产物: {}/ga_generations.csv, {}/ga_detail.csv, {}/best_genome.toml, {}/preset_baseline.toml",
        out_dir_rel, out_dir_rel, out_dir_rel, out_dir_rel);

    // 无结果保护：理论上 run() 已保证 best 存在
    let _: GaGenome = report.best_genome;
    Ok(())
}
