from pathlib import Path

path = Path('crates/umasim/tools/data_collection/skill_pt_phase_matrix.rs')
text = path.read_text(encoding='utf-8')
old = '''    fn new(pt: [u32; 3], sac: u32, extra: &str) -> Result<Self> {
        let suffix = if extra.is_empty() {
            String::new()
        } else {
            format!("-{extra}")
        };
        let make = |p| LocalRamenTrainer::matrix_variant(&format!("pt{p}-sac{sac}-long-fail0{suffix}"));
        Ok(Self {
            years: [make(pt[0])?, make(pt[1])?, make(pt[2])?],
            last: Mutex::new(None)
        })
    }'''
new = '''    fn new(pt: [u32; 3], sac: u32, common: &str, yearly: [&str; 3]) -> Result<Self> {
        let make = |year: usize| {
            let mut tokens = vec![format!("pt{}-sac{sac}-long-fail0", pt[year])];
            if !common.is_empty() {
                tokens.push(common.to_string());
            }
            if !yearly[year].is_empty() {
                tokens.push(yearly[year].to_string());
            }
            LocalRamenTrainer::matrix_variant(&tokens.join("-"))
        };
        Ok(Self {
            years: [make(0)?, make(1)?, make(2)?],
            last: Mutex::new(None)
        })
    }'''
old_main = '''    let extra = env::var("STRUCTURE").unwrap_or_default();
    let shard: u64 = env::var("SHARD").unwrap_or_else(|_| "0".into()).parse()?;'''
new_main = '''    let extra = env::var("STRUCTURE").unwrap_or_default();
    let yearly = [
        env::var("STRUCTURE_Y1").unwrap_or_default(),
        env::var("STRUCTURE_Y2").unwrap_or_default(),
        env::var("STRUCTURE_Y3").unwrap_or_default()
    ];
    let shard: u64 = env::var("SHARD").unwrap_or_else(|_| "0".into()).parse()?;'''
old_calls = '''    let validation = PhaseTrainer::new(pt, sac, &extra).context("分阶段策略参数验证失败")?;
    drop(validation);'''
new_calls = '''    let yearly_refs = [yearly[0].as_str(), yearly[1].as_str(), yearly[2].as_str()];
    let validation = PhaseTrainer::new(pt, sac, &extra, yearly_refs).context("分阶段策略参数验证失败")?;
    drop(validation);'''
old_run = '''        let b = run(PhaseTrainer::new(pt, sac, &extra)?, i)?;'''
new_run = '''        let b = run(PhaseTrainer::new(pt, sac, &extra, yearly_refs)?, i)?;'''
for before, after, label in [
    (old, new, 'constructor'),
    (old_main, new_main, 'environment'),
    (old_calls, new_calls, 'validation'),
    (old_run, new_run, 'runner'),
]:
    if text.count(before) != 1:
        raise SystemExit(f'{label} anchor missing or duplicated')
    text = text.replace(before, after)
path.write_text(text, encoding='utf-8')
