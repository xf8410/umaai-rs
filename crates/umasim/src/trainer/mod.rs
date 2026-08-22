use std::{cell::RefCell, collections::VecDeque, rc::Rc};
use anyhow::Result;
#[cfg(feature = "cli")]
use inquire::Select;
use log::info;
use rand::{Rng, prelude::StdRng, seq::SliceRandom};
use crate::{game::{ActionEnum, BaseAction, Game, Trainer},gamedata::EventChoice};
pub mod handwritten_trainer;
pub mod local_ramen_trainer;
pub mod logging_trainer;
pub mod mcts_trainer;
pub mod ramen_handwritten_trainer;
pub use handwritten_trainer::HandwrittenTrainer;
pub use local_ramen_trainer::LocalRamenTrainer;
pub use logging_trainer::LoggingTrainer;
pub use mcts_trainer::MctsTrainer;
pub use ramen_handwritten_trainer::RamenHandwrittenTrainer;
pub struct RandomTrainer;
impl<G:Game> Trainer<G> for RandomTrainer{
fn select_action(&self,game:&G,actions:&[G::Action],rng:&mut StdRng)->Result<usize>{let mut ids:Vec<_>=(0..actions.len()).collect();ids.shuffle(rng);let mut ret=None;for&i in &ids{if game.uma().vital<45&&actions[i].as_base_action()==Some(BaseAction::Sleep){ret=Some(i);break}else if game.uma().motivation<5&&matches!(actions[i].as_base_action(),Some(BaseAction::NormalOuting)|Some(BaseAction::FriendOuting)){ret=Some(i);break}else if game.uma().vital>=45&&game.uma().motivation>=5&&matches!(actions[i].as_base_action(),Some(BaseAction::Train(_))){ret=Some(i);break}}let i=ret.unwrap_or(ids[0]);info!("吗喽训练员选择：{:?}",actions[i]);Ok(i)}
fn select_choice(&self,_:&G,c:&[Vec<EventChoice>],rng:&mut StdRng)->Result<usize>{Ok(rng.random_range(0..c.len()))}}
pub struct ManualTrainer{pub mock_inputs:Rc<RefCell<VecDeque<String>>>,pub fallback:FallbackMode}
#[derive(Debug,Clone,Copy,PartialEq,Eq)]pub enum FallbackMode{Interactive,PickFirst}
impl Default for ManualTrainer{fn default()->Self{Self::new()}}
impl ManualTrainer{pub fn new()->Self{Self{mock_inputs:Rc::new(RefCell::new(VecDeque::new())),fallback:FallbackMode::Interactive}}pub fn with_mock_inputs(i:Vec<String>)->Self{Self{mock_inputs:Rc::new(RefCell::new(i.into_iter().collect())),fallback:FallbackMode::PickFirst}}fn pop(&self)->Option<String>{self.mock_inputs.borrow_mut().pop_front()}fn first(&self,n:usize)->Result<usize>{if n==0{Err(anyhow::anyhow!("候选为空"))}else{Ok(0)}}}
impl<G:Game> Trainer<G> for ManualTrainer{
fn select_action(&self,_:&G,a:&[G::Action],_:&mut StdRng)->Result<usize>{if let Some(x)=self.pop(){return a.iter().position(|v|v.to_string()==x).ok_or_else(||anyhow::anyhow!("mock输入未匹配"))}match self.fallback{FallbackMode::PickFirst=>self.first(a.len()),#[cfg(feature="cli")]FallbackMode::Interactive=>{let s=Select::new("请选择:",a.to_vec()).with_page_size(a.len()).prompt()?;a.iter().position(|x|*x==s).ok_or_else(||anyhow::anyhow!("未找到动作"))},#[cfg(not(feature="cli"))]FallbackMode::Interactive=>Err(anyhow::anyhow!("需要cli feature"))}}
fn select_choice(&self,_:&G,c:&[Vec<EventChoice>],_:&mut StdRng)->Result<usize>{let e=c.iter().map(|x|x.iter().map(|y|y.explain()).collect::<Vec<_>>().join(" | ")).collect::<Vec<_>>();if let Some(x)=self.pop(){return e.iter().position(|v|v==&x).ok_or_else(||anyhow::anyhow!("mock输入未匹配"))}match self.fallback{FallbackMode::PickFirst=>self.first(e.len()),#[cfg(feature="cli")]FallbackMode::Interactive=>{let s=Select::new("请选择:",e.clone()).prompt()?;e.iter().position(|x|x==&s).ok_or_else(||anyhow::anyhow!("未找到选项"))},#[cfg(not(feature="cli"))]FallbackMode::Interactive=>Err(anyhow::anyhow!("需要cli feature"))}}}
