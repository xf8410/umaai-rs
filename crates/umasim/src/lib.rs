pub mod card_pool;
pub mod bench;
pub mod collector;
pub mod explain;
pub mod game;
pub mod gamedata;
pub mod genetic_optimizer;
pub mod neural;
pub mod output;
pub mod rng;
pub mod sample_collector;
pub mod sampler;
pub mod score_explain;
pub mod search;
pub mod trainer;
pub mod training_sample;
pub mod utils;

// GA 优化器对外入口（遗传算法参数优化，见 genetic_optimizer 模块文档）。
pub use genetic_optimizer::{GaGenome, GaOptimizer, GaParams, GaReport};
