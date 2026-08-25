//! 101种配卡结构分别测试120个拉面地区排列组合；由CI按配卡和地区分片运行。
use std::{env, path::Path};
use anyhow::{Context, Result, ensure};
use rand::prelude::StdRng;
use serde::Serialize;
use umasim::{bench::{self, CardPickOpts, DeckComposition}, game::{Game, InheritInfo, Trainer, ramen::{Operation, RamenAction, RamenGame, rules::get_region_combinations}}, gamedata::{EventChoice, EventData, init_global_with_config}, trainer::RamenHandwrittenTrainer, utils::{get_workspace_root, load_game_config}};
const UMA:u32=102601; const FRIEND:u32=303054;
const INHERIT:InheritInfo=InheritInfo{blue_count:[12,0,0,0,6],extra_count:[10,0,0,20,20,40]};
const BASE_SEED:u64=2_026_082_500;
#[derive(Serialize)] struct Row{composition_index:usize,composition:String,deck:String,combo_index:usize,region_ids:String,run:usize,score:i32,skill_pt:i32,scenario_pt:i32,rmj_success:usize,speed:i32,stamina:i32,power:i32,guts:i32,wisdom:i32}
struct FixedY3{inner:RamenHandwrittenTrainer,combo:[usize;3]}
impl Trainer<RamenGame> for FixedY3{
 fn select_action(&self,game:&RamenGame,actions:&[RamenAction],rng:&mut StdRng)->Result<usize>{if game.turn()==47&&actions.iter().all(|a|matches!(a.operation,Operation::RegionSelect(_))){return actions.iter().position(|a|a.operation==Operation::RegionSelect(self.combo)).ok_or_else(||anyhow::anyhow!("第三年组合不在候选集中: {:?}",self.combo));}self.inner.select_action(game,actions,rng)}
 fn select_choice(&self,g:&RamenGame,c:&[Vec<EventChoice>],r:&mut StdRng)->Result<usize>{self.inner.select_choice(g,c,r)}
 fn select_event_choice(&self,g:&RamenGame,e:&EventData,c:&[Vec<EventChoice>],r:&mut StdRng)->Result<usize>{self.inner.select_event_choice(g,e,c,r)}
 fn last_breakdown(&self)->Option<String>{self.inner.last_breakdown()}
}
fn compositions()->Vec<DeckComposition>{let mut o=Vec::new();for a in 0..=3{for b in 0..=3{for c in 0..=3{for d in 0..=3{for e in 0..=3{let counts=[a,b,c,d,e];if counts.iter().sum::<usize>()==5{o.push(DeckComposition{counts,name:String::new()});}}}}}}o}
fn main()->Result<()>{env::set_current_dir(get_workspace_root()?)?;let config=load_game_config()?;init_global_with_config(&config)?;let ci:usize=env::var("COMPOSITION_INDEX").context("缺少 COMPOSITION_INDEX")?.parse()?;let start:usize=env::var("COMBO_START").unwrap_or_else(|_|"0".into()).parse()?;let end:usize=env::var("COMBO_END").unwrap_or_else(|_|"120".into()).parse()?;let runs:usize=env::var("RUNS").unwrap_or_else(|_|"1".into()).parse()?;let cs=compositions();ensure!(cs.len()==101,"配卡构成数量不是101: {}",cs.len());let comp=cs.get(ci).with_context(||format!("配卡构成索引越界: {ci}"))?;let reps=bench::select_representatives(&CardPickOpts::default())?;let deck=comp.build_deck(&reps.picked,FRIEND)?;let combos=get_region_combinations(2)?;ensure!(combos.len()==120,"地区组合数量不是120: {}",combos.len());ensure!(start<end&&end<=120,"地区分片无效: {start}..{end}");let mut w=csv::Writer::from_path(Path::new("composition-y3-matrix.csv"))?;let dt=deck.iter().map(u32::to_string).collect::<Vec<_>>().join("/");for(combo_index,combo)in combos[start..end].iter().enumerate(){let combo_index=start+combo_index;for run in 0..runs{let(mut rng,rule_master)=bench::seeded_rngs(BASE_SEED,(ci*120*runs+combo_index*runs+run)as u64);let mut game=RamenGame::newgame(UMA,&deck,INHERIT)?;game.set_rule_master(rule_master);let trainer=FixedY3{inner:RamenHandwrittenTrainer::new(),combo:*combo};game.run_full_game(&trainer,&mut rng)?;let s=game.uma.five_status;w.serialize(Row{composition_index:ci,composition:comp.name(),deck:dt.clone(),combo_index,region_ids:combo.iter().map(usize::to_string).collect::<Vec<_>>().join("/"),run,score:game.uma.calc_score(),skill_pt:game.uma.skill_pt,scenario_pt:game.ramen.scenario_pt,rmj_success:game.ramen.rmj_results.iter().filter(|&&x|x).count(),speed:s[0],stamina:s[1],power:s[2],guts:s[3],wisdom:s[4]})?;}}w.flush()?;Ok(())}
