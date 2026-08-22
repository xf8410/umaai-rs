//! 1000 组配对测试：上游手写基准策略与带保护上限的本地修正策略。
use std::{cmp::Ordering, fs};
use anyhow::{Result, ensure};
use umasim::{
    bench::{self, GameOutcome},
    game::{InheritInfo, ramen::rules::calc_ramen_pt_gain},
    gamedata::init_global_with_config,
    output::decision_log::{DecisionLog, DecisionLogRow},
    trainer::{LocalRamenTrainer, LoggingTrainer, RamenHandwrittenTrainer},
    utils::{get_workspace_root, load_game_config}
};
const RUNS:usize=1000; const BASE_SEED:u64=61444; const UMA:u32=102601;
const DECK:[u32;6]=[302424,302894,303044,302924,303024,303054];
const INHERIT:InheritInfo=InheritInfo{blue_count:[15,0,0,0,3],extra_count:[10,10,20,20,20,40]};

fn apply_metrics(o:&mut GameOutcome,rows:&[DecisionLogRow])->Result<()>{
    let mut yearly=[0_i32;3];let mut pt=0;
    for r in rows {if r.stage!="RamenSelect"||!r.action_desc.starts_with("吃面/"){continue;}
        let y=if r.turn<24{0}else if r.turn<48{1}else{2};pt+=calc_ramen_pt_gain(y,yearly[y])?;yearly[y]+=1;}
    let eat=yearly.iter().sum();ensure!(eat>0,"seed={} 未记录吃面",o.seed);ensure!(pt>0,"seed={} RMJ点为0",o.seed);
    o.scenario_pt=pt;o.eat_count=eat;Ok(())
}
fn comparison(a:i32,b:i32)->String{match a.cmp(&b){Ordering::Less=>format!("A比B少{}分",b-a),Ordering::Greater=>format!("B比A少{}分",a-b),Ordering::Equal=>"A与B同分".into()}}
fn summary(label:&str,v:&[GameOutcome]){
    let x=v.iter().map(|o|o.score as f64).collect::<Vec<_>>();let s=bench::summarize(&x);let n=v.len() as f64;
    let rmj=v.iter().map(|o|o.rmj_ok as f64).sum::<f64>()/n;let pt=v.iter().map(|o|o.scenario_pt as f64).sum::<f64>()/n;
    let sp=v.iter().map(|o|o.skill_pt as f64).sum::<f64>()/n;let eat=v.iter().map(|o|o.eat_count as f64).sum::<f64>()/n;
    println!("RESULT {label}: 局数={} 平均评分={:.0} 中位评分={:.0} 最低评分={:.0} 最高评分={:.0} 评分总体标准差={:.0} 平均RMJ成功={:.2}/3年 平均累计RMJ剧本点={:.0} 平均最终技能点={:.0} 平均每局吃面={:.1}碗",v.len(),s.mean,s.median,s.min,s.max,s.std,rmj,pt,sp,eat);
}
fn clean(s:&str)->String{s.replace('\t'," ").replace('\n'," ").replace('\r'," ")}
fn main()->Result<()>{
    let root=get_workspace_root()?;std::env::set_current_dir(root)?;init_global_with_config(&load_game_config()?)?;
    println!("A=上游手写基准策略（RamenPolicy默认配置）。");
    println!("B=同一基准+本地长期收益修正；本地修正最多允许牺牲120点上游基础训练分。");
    println!("差值全部用正数自然语言显示；完整决策日志和首次分歧诊断在CI Artifact中。");
    let(mut ar,mut br)=(Vec::with_capacity(RUNS),Vec::with_capacity(RUNS));
    let(mut a_rows,mut b_rows)=(Vec::new(),Vec::new());
    let mut div=String::from("seed\tfinal_comparison\tturn\tstage\tA_action\tB_action\tA_breakdown\tB_breakdown\n");
    println!("开始 A/B：每套策略 {RUNS} 局，随机种子 {BASE_SEED}..{}",BASE_SEED+RUNS as u64-1);
    for i in 0..RUNS {
        let seed=BASE_SEED+i as u64;let ta=LoggingTrainer::new(RamenHandwrittenTrainer::new(),seed);let tb=LoggingTrainer::new(LocalRamenTrainer::new(),seed);
        let mut a=bench::run_seeded(UMA,&DECK,&INHERIT,seed,&ta)?;let mut b=bench::run_seeded(UMA,&DECK,&INHERIT,seed,&tb)?;
        let al=ta.take_records();let bl=tb.take_records();apply_metrics(&mut a,&al.rows)?;apply_metrics(&mut b,&bl.rows)?;
        let cmp=comparison(a.score,b.score);
        if let Some((x,y))=al.rows.iter().zip(bl.rows.iter()).find(|(x,y)|x.turn!=y.turn||x.stage!=y.stage||x.action_desc!=y.action_desc){
            div.push_str(&format!("{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\n",seed,cmp,x.turn,clean(&x.stage),clean(&x.action_desc),clean(&y.action_desc),clean(x.score_breakdown.as_deref().unwrap_or("")),clean(y.score_breakdown.as_deref().unwrap_or(""))));
        }else{div.push_str(&format!("{seed}\t{cmp}\t-\t无分歧\t-\t-\t-\t-\n"));}
        println!("seed={seed} | A(上游手写基准)[评分={}，RMJ成功={}/3年，累计RMJ剧本点={}，最终技能点={}，全局吃面={}碗] | B(基准+受限本地修正)[评分={}，RMJ成功={}/3年，累计RMJ剧本点={}，最终技能点={}，全局吃面={}碗] | {cmp}",a.score,a.rmj_ok,a.scenario_pt,a.skill_pt,a.eat_count,b.score,b.rmj_ok,b.scenario_pt,b.skill_pt,b.eat_count);
        a_rows.extend(al.rows);b_rows.extend(bl.rows);ar.push(a);br.push(b);
    }
    fs::create_dir_all("logs")?;DecisionLog{rows:a_rows}.save_to(std::path::Path::new("logs/A_upstream_decisions.csv"))?;
    DecisionLog{rows:b_rows}.save_to(std::path::Path::new("logs/B_local_decisions.csv"))?;fs::write("logs/first_divergence.tsv",div)?;
    summary("A(上游手写基准)",&ar);summary("B(基准+受限本地修正)",&br);
    let diffs=ar.iter().zip(&br).map(|(a,b)|b.score-a.score).collect::<Vec<_>>();
    let aw=diffs.iter().filter(|&&d|d<0).count();let bw=diffs.iter().filter(|&&d|d>0).count();let ties=RUNS-aw-bw;
    let mean=diffs.iter().map(|&d|d as f64).sum::<f64>()/RUNS as f64;let mut sorted=diffs.clone();sorted.sort();
    let median=(sorted[(RUNS-1)/2]+sorted[RUNS/2]) as f64/2.0;let bad=diffs.iter().filter(|&&d|d<=-3000).count();let good=diffs.iter().filter(|&&d|d>=3000).count();
    println!("PAIRED B胜={}局 A胜={}局 同分={}局 B胜率={:.1}% 配对差值中位数={median:.0}分 B大胜(≥3000)={}局 B大败(≤-3000)={}局",bw,aw,ties,bw as f64/RUNS as f64*100.0,good,bad);
    println!("DELTA 平均评分比较：{}",comparison(0,mean.round() as i32));
    Ok(())
}
