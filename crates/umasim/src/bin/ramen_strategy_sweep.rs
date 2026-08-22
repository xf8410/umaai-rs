//! 单参数策略扫描。由 CI matrix 为每个配置单独运行。
use std::env;
use anyhow::Result;
use umasim::{
    bench,
    game::InheritInfo,
    gamedata::init_global_with_config,
    trainer::{LocalRamenConfig, LocalRamenTrainer, RamenHandwrittenTrainer},
    utils::{get_workspace_root, load_game_config}
};
const UMA:u32=102601;const DECK:[u32;6]=[302424,302894,303044,302924,303024,303054];
const INHERIT:InheritInfo=InheritInfo{blue_count:[15,0,0,0,3],extra_count:[10,10,20,20,20,40]};
fn setting(name:&str)->Result<LocalRamenConfig>{let mut c=LocalRamenConfig::default();match name{
"baseline"=>{},"sac80"=>c.max_base_score_sacrifice=80.0,"sac100"=>c.max_base_score_sacrifice=100.0,"sac140"=>c.max_base_score_sacrifice=140.0,
"fail500"=>c.high_fail_penalty=500.0,"fail900"=>c.high_fail_penalty=900.0,
"bond7"=>c.early_bond_value=7.0,"bond9"=>c.early_bond_value=9.0,
"friend75"=>c.first_friend_click_value=75.0,"friend105"=>c.first_friend_click_value=105.0,
"no_overflow"=>c.overflow_value=0.0,"no_urgency"=>c.rmj_urgency_bonus=0.0,
"hint4"=>c.hint_bonus=4.0,"hint8"=>c.hint_bonus=8.0,
_=>anyhow::bail!("unknown variant: {name}")};Ok(c)}
fn median(mut x:Vec<i32>)->f64{x.sort();let n=x.len();if n%2==0{(x[n/2-1]+x[n/2])as f64/2.0}else{x[n/2]as f64}}
fn main()->Result<()>{let variant=env::var("VARIANT").unwrap_or_else(|_|"baseline".into());let start=env::var("BASE_SEED").ok().and_then(|x|x.parse().ok()).unwrap_or(61444_u64);let runs=env::var("RUNS").ok().and_then(|x|x.parse().ok()).unwrap_or(1000_usize);let root=get_workspace_root()?;std::env::set_current_dir(root)?;init_global_with_config(&load_game_config()?)?;let cfg=setting(&variant)?;let a=RamenHandwrittenTrainer::new();let b=LocalRamenTrainer::with_config(cfg);let mut diffs=Vec::with_capacity(runs);let(mut asum,mut bsum)=(0_i64,0_i64);for i in 0..runs{let seed=start+i as u64;let ao=bench::run_seeded(UMA,&DECK,&INHERIT,seed,&a)?;let bo=bench::run_seeded(UMA,&DECK,&INHERIT,seed,&b)?;asum+=ao.score as i64;bsum+=bo.score as i64;diffs.push(bo.score-ao.score)}let bw=diffs.iter().filter(|&&d|d>0).count();let aw=diffs.iter().filter(|&&d|d<0).count();let ties=runs-bw-aw;let bigw=diffs.iter().filter(|&&d|d>=3000).count();let bigl=diffs.iter().filter(|&&d|d<=-3000).count();let delta=(bsum-asum)as f64/runs as f64;println!("SWEEP variant={variant} seeds={start}..{} runs={runs} A_mean={:.1} B_mean={:.1} delta={delta:.1} median_delta={:.1} B_win={bw} A_win={aw} tie={ties} big_win={bigw} big_loss={bigl}",start+runs as u64-1,asum as f64/runs as f64,bsum as f64/runs as f64,median(diffs));Ok(())}
