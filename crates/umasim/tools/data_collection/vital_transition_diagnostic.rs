//! P0 第3步结构诊断：记录吃面训练后的体力与下一回合恢复行为。

use std::{env, path::Path, sync::Mutex};
use anyhow::Result;
use rand::prelude::StdRng;
use umasim::{
    bench::{self, CardPickOpts, DeckComposition},
    game::{Game, InheritInfo, Trainer, ramen::{Operation, RamenAction, RamenGame, RamenStage}},
    gamedata::{EventChoice, EventData, init_global_with_config},
    trainer::RecommendedRamenTrainer,
    utils::{get_workspace_root, load_game_config},
};

const UMA: u32 = 102601;
const FRIEND: u32 = 303054;
const INHERIT: InheritInfo = InheritInfo { blue_count: [15,0,0,0,3], extra_count: [10,10,20,20,20,40] };

#[derive(Default, Clone)]
struct Stats {
    eaten_trains: i32,
    post_below_0: i32,
    post_below_10: i32,
    post_below_15: i32,
    post_below_20: i32,
    immediate_gain: i32,
    immediate_pt: i32,
    next_observed: i32,
    next_rest: i32,
    next_friend: i32,
    next_train: i32,
    pending_eat_turn: Option<i32>,
}

struct DiagnosticTrainer {
    inner: RecommendedRamenTrainer,
    stats: Mutex<Stats>,
}

impl DiagnosticTrainer {
    fn new(name: &str) -> Result<Self> {
        let args = match name {
            "无转移预算" => (0, 0, 0.0, 0, true),
            "训练后10轻罚" => (0, 10, 2.0, 0, true),
            "无恢复视野后15" => (0, 15, 2.0, 0, false),
            "训练后硬底线0" => (0, 0, 0.0, 1, true),
            _ => anyhow::bail!("未知候选: {name}"),
        };
        Ok(Self {
            inner: RecommendedRamenTrainer::with_vital_transition_overrides(args.0,args.1,args.2,args.3,args.4),
            stats: Mutex::new(Stats::default()),
        })
    }
    fn snapshot(&self) -> Stats { self.stats.lock().map(|x| x.clone()).unwrap_or_default() }
}

impl Trainer<RamenGame> for DiagnosticTrainer {
    fn select_action(&self, game: &RamenGame, actions: &[RamenAction], rng: &mut StdRng) -> Result<usize> {
        let idx = self.inner.select_action(game, actions, rng)?;
        if game.current_year() == 3 && game.stage == RamenStage::Train {
            let action = actions.get(idx).ok_or_else(|| anyhow::anyhow!("动作索引越界"))?;
            let mut s = self.stats.lock().map_err(|_| anyhow::anyhow!("诊断锁损坏"))?;
            if s.pending_eat_turn.is_some_and(|turn| game.turn() > turn) {
                s.next_observed += 1;
                match action.operation {
                    Operation::Rest => s.next_rest += 1,
                    Operation::FriendOuting => s.next_friend += 1,
                    Operation::Train(_) => s.next_train += 1,
                    _ => {}
                }
                s.pending_eat_turn = None;
            }
            if game.ramen.current_ramen.is_some() {
                if let Operation::Train(tt) = action.operation {
                    let train = tt as usize;
                    let buffs = game.calc_training_buff(train)?;
                    let value = game.calc_training_value(&buffs, train)?;
                    let post = game.uma.vital + value.vital;
                    s.eaten_trains += 1;
                    s.post_below_0 += i32::from(post < 0);
                    s.post_below_10 += i32::from(post < 10);
                    s.post_below_15 += i32::from(post < 15);
                    s.post_below_20 += i32::from(post < 20);
                    s.immediate_gain += value.status_pt[..5].iter().sum::<i32>() + value.status_pt[5] * 2;
                    s.immediate_pt += value.status_pt[5];
                    s.pending_eat_turn = Some(game.turn());
                }
            }
        }
        Ok(idx)
    }
    fn select_choice(&self, game: &RamenGame, choices: &[Vec<EventChoice>], rng: &mut StdRng) -> Result<usize> {
        self.inner.select_choice(game, choices, rng)
    }
    fn select_event_choice(&self, game: &RamenGame, event: &EventData, choices: &[Vec<EventChoice>], rng: &mut StdRng) -> Result<usize> {
        self.inner.select_event_choice(game, event, choices, rng)
    }
}

fn deck() -> Result<[u32;6]> {
    let c=DeckComposition{counts:[3,1,0,0,1],name:String::new()};
    let r=bench::select_representatives(&CardPickOpts::default())?;
    c.build_deck(&r.picked,FRIEND)
}

fn main() -> Result<()> {
    std::env::set_current_dir(get_workspace_root()?)?;
    init_global_with_config(&load_game_config()?)?;
    let name=env::var("候选方案")?; let seed:u64=env::var("基础种子")?.parse()?;
    let shard:u64=env::var("分片序号")?.parse()?; let runs:u64=env::var("每分片局数")?.parse()?;
    let deck=deck()?; let mut rows=Vec::with_capacity(runs as usize);
    for offset in 0..runs {
        let run_index=shard*runs+offset; let (mut rng,master)=bench::seeded_rngs(seed,run_index);
        let mut game=RamenGame::newgame(UMA,&deck,INHERIT.clone())?; game.set_rule_master(master);
        let trainer=DiagnosticTrainer::new(&name)?; game.run_full_game(&trainer,&mut rng)?; let s=trainer.snapshot();
        rows.push(vec![name.clone(),run_index.to_string(),game.uma.calc_score().to_string(),game.uma.skill_pt.to_string(),
            s.eaten_trains.to_string(),s.post_below_0.to_string(),s.post_below_10.to_string(),s.post_below_15.to_string(),s.post_below_20.to_string(),
            s.immediate_gain.to_string(),s.immediate_pt.to_string(),s.next_observed.to_string(),s.next_rest.to_string(),s.next_friend.to_string(),s.next_train.to_string()]);
    }
    bench::write_csv(Path::new("吃后体力转移结构.csv"),
        &["方案","局序号","总分","技能点","吃面训练","吃后负体力","吃后低于10","吃后低于15","吃后低于20","吃面即时收益","吃面即时PT","次回合已观察","次回合休息","次回合友人","次回合训练"],&rows)
}
