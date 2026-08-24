from pathlib import Path

p = Path('crates/umasim/src/bin/bench_compositions.rs')
s = p.read_text(encoding='utf-8')

def one(old, new):
    global s
    if s.count(old) != 1:
        raise SystemExit(f'expected one match, got {s.count(old)}: {old[:120]!r}')
    s = s.replace(old, new)

one(
'''    trainer::{LoggingTrainer, RamenHandwrittenTrainer, RandomTrainer},''',
'''    trainer::{LoggingTrainer, RamenHandwrittenTrainer, RandomTrainer, RecommendedRamenTrainer},''')

one(
'''const DEFAULT_INHERIT: InheritInfo = InheritInfo {
    blue_count: [12, 0, 0, 0, 6],
    extra_count: [10, 0, 0, 20, 20, 40]
};''',
'''const DEFAULT_INHERIT: InheritInfo = InheritInfo {
    blue_count: [15, 0, 0, 0, 3],
    extra_count: [10, 10, 20, 20, 20, 40]
};''')

one(
'''        matches!(cfg.trainer.as_str(), "random" | "handwritten"),
        "--trainer 只能为 random 或 handwritten"''',
'''        matches!(cfg.trainer.as_str(), "random" | "handwritten" | "recommended"),
        "--trainer 只能为 random、handwritten 或 recommended"''')

one(
'''    let random = |seed: u64| LoggingTrainer::new(RandomTrainer, seed);
    let handwritten = |seed: u64| LoggingTrainer::new(RamenHandwrittenTrainer::new(), seed);
    let mut summaries = Vec::with_capacity(compositions.len());''',
'''    let random = |seed: u64| LoggingTrainer::new(RandomTrainer, seed);
    let handwritten = |seed: u64| LoggingTrainer::new(RamenHandwrittenTrainer::new(), seed);
    let recommended = |seed: u64| LoggingTrainer::new(RecommendedRamenTrainer::new(), seed);
    let mut summaries = Vec::with_capacity(compositions.len());''')

one(
'''            "handwritten" => run_composition(cfg, composition, deck, &handwritten),
            _ => unreachable!("训练员已在参数解析时校验")''',
'''            "handwritten" => run_composition(cfg, composition, deck, &handwritten),
            "recommended" => run_composition(cfg, composition, deck, &recommended),
            _ => unreachable!("训练员已在参数解析时校验")''')

p.write_text(s, encoding='utf-8')
