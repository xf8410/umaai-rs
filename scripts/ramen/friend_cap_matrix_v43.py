from pathlib import Path

p = Path('crates/umasim/tools/data_collection/skill_pt_phase_matrix.rs')
s = p.read_text(encoding='utf-8')

def one(old, new):
    global s
    if s.count(old) != 1:
        raise SystemExit(f'expected one match, got {s.count(old)}: {old[:120]!r}')
    s = s.replace(old, new)

one(
'''    trainer::{LocalRamenTrainer, LoggingTrainer, RamenHandwrittenTrainer},''',
'''    trainer::{LocalRamenTrainer, LoggingTrainer, RamenHandwrittenTrainer},''')

one(
'''    fn new(pt: [u32; 3], sac: u32, common: &str, yearly: [&str; 3]) -> Result<Self> {''',
'''    fn new(pt: [u32; 3], sac: u32, common: &str, yearly: [&str; 3], suffix: &str) -> Result<Self> {''')

one(
'''            if !yearly[year].is_empty() {
                tokens.push(yearly[year].to_string());
            }
            LocalRamenTrainer::matrix_variant(&tokens.join("-"))''',
'''            if !yearly[year].is_empty() {
                tokens.push(yearly[year].to_string());
            }
            if !suffix.is_empty() {
                tokens.push(suffix.to_string());
            }
            LocalRamenTrainer::matrix_variant(&tokens.join("-"))''')

one(
'''    let validation = PhaseTrainer::new(pt, sac, &extra, yearly_refs).context("分阶段策略参数验证失败")?;''',
'''    let cap_a = env::var("FRIEND_CAP_A").unwrap_or_default();
    let cap_b = env::var("FRIEND_CAP_B").unwrap_or_default();
    let validation = PhaseTrainer::new(pt, sac, &extra, yearly_refs, &cap_b).context("分阶段策略参数验证失败")?;''')

one(
'''        let a = run(RamenHandwrittenTrainer::new(), i)?;
        let b = run(PhaseTrainer::new(pt, sac, &extra, yearly_refs)?, i)?;''',
'''        let a = if cap_a.is_empty() {
            run(RamenHandwrittenTrainer::new(), i)?
        } else {
            run(PhaseTrainer::new(pt, sac, &extra, yearly_refs, &cap_a)?, i)?
        };
        let b = run(PhaseTrainer::new(pt, sac, &extra, yearly_refs, &cap_b)?, i)?;''')

p.write_text(s, encoding='utf-8')
