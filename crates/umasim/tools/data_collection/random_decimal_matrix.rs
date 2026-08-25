//! 修正训练公式后的多维小数权重矩阵；参数、配卡及第三年地区由 CI 显式传入。
use std::{env,path::Path};
use anyhow::{Context,Result};
use rand::prelude::StdRng;
use umasim::{bench::{self,CardPickOpts,DeckComposition},game::{Game,InheritInfo,Trainer,ramen::{Operation,RamenAction,RamenGame}},gamedata::{EventChoice,EventData,init_global_with_config},trainer::RecommendedRamenTrainer,utils::{get_workspace_root,load_game_config}};
const UMA:u32=102601;const FRIEND:u32=303054;
const INHERIT:InheritInfo=InheritInfo{blue_count:[15,0,0,0,3],extra_count:[10,10,20,20,20,40]};
fn f(k:&str)->Result<f32>{Ok(env::var(k)?.parse()?)}
fn trainer()->Result<RecommendedRamenTrainer>{Ok(RecommendedRamenTrainer::with_random_decimal_overrides([f("Y1_PT")?,f("Y2_PT")?,f("Y3_PT")?],f("GAP")?,f("OVER")?,f("SAC")?,f("Y12_WINDOW")?,f("Y3_WINDOW")?,f("RESERVE")?,f("BOND")?,f("HINT")?,f("CONTINUITY")?))}
fn counts()->Result<[usize;5]>{match env::var("BUILD")?.as_str(){"3速1耐1智"=>Ok([3,1,0,0,1]),"2速2耐1智"=>Ok([2,2,0,0,1]),x=>anyhow::bail!("未知配卡: {x}")}}
fn fixed()->Result<[usize;3]>{let v=env::var("Y3_REGION")?.split('/').map(str::parse).collect::<std::result::Result<Vec<usize>,_>>()?;v.try_into().map_err(|_|anyhow::anyhow!("Y3_REGION必须为三个ID"))}
struct FixedY3{inner:RecommendedRamenTrainer,combo:[usize;3]}
impl Trainer<RamenGame> for FixedY3{
 fn select_action(&self,g:&RamenGame,a:&[RamenAction],r:&mut StdRng)->Result<usize>{if g.turn()==47&&a.iter().all(|x|matches!(x.operation,Operation::RegionSelect(_))){return a.iter().position(|x|x.operation==Operation::RegionSelect(self.combo)).with_context(||format!("缺少地区{:?}",self.combo));}self.inner.select_action(g,a,r)}
 fn select_choice(&self,g:&RamenGame,c:&[Vec<EventChoice>],r:&mut StdRng)->Result<usize>{self.inner.select_choice(g,c,r)}
 fn select_event_choice(&self,g:&RamenGame,e:&EventData,c:&[Vec<EventChoice>],r:&mut StdRng)->Result<usize>{self.inner.select_event_choice(g,e,c,r)}
}
fn main()->Result<()>{env::set_current_dir(get_workspace_root()?)?;init_global_with_config(&load_game_config()?)?;let id=env::var("候选编号")?;let build=env::var("BUILD")?;let region=fixed()?;let seed:u64=env::var("基础种子")?.parse()?;let shard:u64=env::var("分片序号")?.parse()?;let runs:u64=env::var("每分片局数")?.parse()?;let reps=bench::select_representatives(&CardPickOpts::default())?;let deck=DeckComposition{counts:counts()?,name:String::new()}.build_deck(&reps.picked,FRIEND)?;let params=["Y1_PT","Y2_PT","Y3_PT","GAP","OVER","SAC","Y12_WINDOW","Y3_WINDOW","RESERVE","BOND","HINT","CONTINUITY"].map(|k|env::var(k).unwrap());let mut rows=Vec::with_capacity(runs as usize);for off in 0..runs{let index=shard*runs+off;let(mut rng,master)=bench::seeded_rngs(seed,index);let mut g=RamenGame::newgame(UMA,&deck,INHERIT)?;g.set_rule_master(master);g.run_full_game(&FixedY3{inner:trainer()?,combo:region},&mut rng)?;let mut row=vec![id.clone(),build.clone(),env::var("Y3_REGION")?,seed.to_string(),index.to_string(),g.uma.calc_score().to_string(),g.uma.skill_pt.to_string()];row.extend(params.clone());rows.push(row);}bench::write_csv(Path::new("随机小数权重矩阵.csv"),&["候选","配卡","第三年地区","基础种子","局序号","总分","技能点","Y1PT","Y2PT","Y3PT","缺口","溢出","让分","前两年对盘","第三年对盘","预留","羁绊","Hint","续航"],&rows)}
