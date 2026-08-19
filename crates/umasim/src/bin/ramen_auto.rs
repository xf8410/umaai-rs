//! 拉面杯自动策略模拟器
//!
//! 使用 `RamenTrainer`（自动启发式策略）跑一局完整的拉面杯，
//! 输出完整日志（训练过程、事件、最终评分）。
//!
//! 适用于 CI 环境自动测试和策略效果验证。
//!
//! # 启动方式
//!
//! ```bash
//! # 使用默认种子
//! cargo run --bin ramen_auto --release
//!
//! # 指定种子
//! SIM_SEED=42 cargo run --bin ramen_auto --release
//! ```
//!
//! # 配置
//!
//! 启动时会读取 `game_config.toml`（参考 `gamedata/default_config.toml`）。
//! 本程序**只使用以下字段**：
//!
//! - `log_level`：日志级别
//! - `uma`：马娘 ID
//! - `cards`：6 张支援卡 ID
//! - `extra_count`：种马额外属性
//!
//! 本程序**强制要求**：
//!
//! - `scenario = "ramen"`
//!
//! SEED 可通过环境变量 `SIM_SEED` 设置，默认 20240816。

use std::time::Instant;

use anyhow::{Result, anyhow};
use log::info;
use rand::{SeedableRng, rngs::StdRng};

use umasim::{
    game::{
        Game, InheritInfo,
        ramen::RamenGame,
    },
    gamedata::{GAMECONSTANTS, init_global_with_config},
    global,
    trainer::RamenTrainer,
    utils::{init_logger_stdout, load_game_config},
};

fn main() -> Result<()> {
    // 1. 加载配置
    let game_config = load_game_config()?;

    // 2. 校验剧本类型
    if game_config.scenario != "ramen" {
        return Err(anyhow!(
            "ramen_auto 要求 scenario = \"ramen\"，当前 game_config.toml 中为 {:?}\n\
            请修改 game_config.toml：scenario = \"ramen\"",
            game_config.scenario
        ));
    }

    // 3. 从环境变量获取种子
    let seed: u64 = std::env::var("SIM_SEED")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(20240816);

    // 4. 初始化日志和全局数据
    init_logger_stdout("ramen_auto", &game_config.log_level)?;
    init_global_with_config(&game_config)?;

    // 5. 提取配置
    let uma_id = game_config.uma;
    let deck = game_config.cards;
    let inherit = InheritInfo {
        blue_count: game_config.blue_count,
        extra_count: game_config.extra_count,
    };

    println!("╔══════════════════════════════════════════════╗");
    println!("║        拉面杯 RamenTrainer 自动模拟器         ║");
    println!("╚══════════════════════════════════════════════╝");
    println!();
    println!("马娘: {}", uma_id);
    println!("卡组: {:?}", deck);
    println!("继承: blue={:?} extra={:?}", inherit.blue_count, inherit.extra_count);
    println!("种子: {}", seed);
    println!("日志: {}", game_config.log_level);
    println!();

    // 6. 初始化游戏
    let mut rng = StdRng::seed_from_u64(seed);
    let mut game = RamenGame::newgame(uma_id, &deck, inherit)?;
    let trainer = RamenTrainer::new().verbose(true);

    println!("=== 开局状态 ===");
    println!("{}", game.explain()?);
    println!();

    // 7. 运行完整游戏
    let start = Instant::now();
    info!("开始 RamenTrainer 自动模拟 (seed={})...", seed);
    game.run_full_game(&trainer, &mut rng)?;
    let elapsed = start.elapsed();

    // 8. 输出结果
    println!();
    println!("╔══════════════════════════════════════════════╗");
    println!("║              育成结束！                       ║");
    println!("╚══════════════════════════════════════════════╝");
    println!();
    println!("最终回合: {} (max_turn={})", game.turn(), game.max_turn());
    println!("剧本PT: {}", game.ramen.scenario_pt);
    println!("RMJ结果: {:?}", game.ramen.rmj_results);
    println!("地区选择: {:?}", game.ramen.selected_regions);
    println!("超级拉面选择: {:?}", game.ramen.super_ramen);
    println!(
        "诀窍库存: A={} B={} C={}",
        game.ramen.feeling_stock[0],
        game.ramen.feeling_stock[1],
        game.ramen.feeling_stock[2]
    );
    println!("隐藏风味: {}", game.ramen.special_feeling);

    // 属性详情
    println!();
    println!("=== 最终属性 ===");
    println!("速度: {}", game.uma.five_status[0]);
    println!("耐力: {}", game.uma.five_status[1]);
    println!("力量: {}", game.uma.five_status[2]);
    println!("根性: {}", game.uma.five_status[3]);
    println!("智力: {}", game.uma.five_status[4]);

    let score = game.uma.calc_score();
    let pt = game.uma.total_pt();
    println!();
    println!(
        "评分: {} {}, PT: {}",
        global!(GAMECONSTANTS).get_rank_name(score),
        score,
        pt
    );
    println!("耗时: {:?}", elapsed);

    // CI 友好的摘要行
    println!();
    println!("SUMMARY: score={} rank={} pt={} seed={} time_ms={}",
        score,
        global!(GAMECONSTANTS).get_rank_name(score),
        pt,
        seed,
        elapsed.as_millis()
    );

    Ok(())
}
