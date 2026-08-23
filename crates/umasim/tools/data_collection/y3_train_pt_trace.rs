//! 逐次记录第三年每个训练动作实际获得的技能 PT。
use std::{env, fs::File, io::Write, sync::Mutex};

use anyhow::{Context, Result};
use rand::prelude::StdRng;
use umasim::{
    bench,
    game::{
        Game,
        InheritInfo,
        Trainer,
        ramen::{Operation, RamenAction, RamenGame, RamenStage}
    },
    gamedata::{EventChoice, EventData, init_global_with_config},
    trainer::LocalRamenTrainer,
    utils::{get_workspace_root, load_game_config}
};

const BASE_SEED: u64 = 61444;
const UMA: u32 = 102601;
const DECK: [u32; 6] = [302424, 302894, 303044, 302924, 303024, 303054];
const INHERIT: InheritInfo = InheritInfo {
    blue_count: [15, 0, 0, 0, 3],
    extra_count: [10, 10, 20, 20, 20, 40]
};
const STRUCTURE: &str = "structall-rpt200-window10-look0-samples1-rawfail-cook240-vrest30-eatguard-friendrest-friend3v45-friendcap135-friendspecial2";

struct PhaseTrainer {
    years: [LocalRamenTrainer; 3],
    selected: Mutex<Option<Operation>>
}
impl PhaseTrainer {
    fn new() -> Result<Self> {
        let make = |pt| LocalRamenTrainer::matrix_variant(&format!("pt{pt}-sac140-long-fail0-{STRUCTURE}"));
        Ok(Self {
            years: [make(16)?, make(64)?, make(64)?],
            selected: Mutex::new(None)
        })
    }
    fn year(g: &RamenGame) -> usize {
        if g.turn() < 24 {
            0
        } else if g.turn() < 48 {
            1
        } else {
            2
        }
    }
    fn take_selected(&self) -> Option<Operation> {
        self.selected.lock().ok()?.take()
    }
}
impl Trainer<RamenGame> for PhaseTrainer {
    fn select_action(&self, g: &RamenGame, a: &[RamenAction], r: &mut StdRng) -> Result<usize> {
        let i = self.years[Self::year(g)].select_action(g, a, r)?;
        if g.stage == RamenStage::Train {
            *self.selected.lock().unwrap() = a.get(i).map(|x| x.operation);
        }
        Ok(i)
    }
    fn select_choice(&self, g: &RamenGame, c: &[Vec<EventChoice>], r: &mut StdRng) -> Result<usize> {
        self.years[Self::year(g)].select_choice(g, c, r)
    }
    fn select_event_choice(
        &self, g: &RamenGame, e: &EventData, c: &[Vec<EventChoice>], r: &mut StdRng
    ) -> Result<usize> {
        self.years[Self::year(g)].select_event_choice(g, e, c, r)
    }
}

fn main() -> Result<()> {
    std::env::set_current_dir(get_workspace_root()?)?;
    init_global_with_config(&load_game_config()?)?;
    let runs: u64 = env::var("RUNS").unwrap_or_else(|_| "100".into()).parse()?;
    let start: u64 = env::var("START").unwrap_or_else(|_| "0".into()).parse()?;
    let mut file = File::create("y3-train-pt.csv")?;
    writeln!(
        file,
        "run_idx,turn,phase,training,success,pt_before,pt_after,pt_gain,vital_before,vital_after,ate_ramen"
    )?;

    for run_idx in start..start + runs {
        let (mut rng, rule_master) = bench::seeded_rngs(BASE_SEED, run_idx);
        let mut game = RamenGame::newgame(UMA, &DECK, INHERIT.clone())?;
        game.set_rule_master(rule_master);
        let trainer = PhaseTrainer::new()?;
        loop {
            let was_train = game.stage == RamenStage::Train;
            let turn = game.turn();
            let pt_before = game.uma.skill_pt;
            let vital_before = game.uma.vital;
            let levels_before = game.base.train_level_count;
            let ate = game.ramen.current_ramen.is_some() || game.is_super_ramen_turn();
            game.run_stage(&trainer, &mut rng)
                .with_context(|| format!("run={run_idx} turn={turn} stage={:?}", game.stage))?;
            if was_train && turn >= 48 {
                if let Some(Operation::Train(tt)) = trainer.take_selected() {
                    let train = tt as usize;
                    let success = game.base.train_level_count[train] > levels_before[train];
                    let phase = if turn < 72 { "Y3" } else { "URA" };
                    let name = ["speed", "stamina", "power", "guts", "wisdom"][train];
                    writeln!(
                        file,
                        "{run_idx},{},{phase},{name},{success},{pt_before},{},{},{vital_before},{},{ate}",
                        turn + 1,
                        game.uma.skill_pt,
                        game.uma.skill_pt - pt_before,
                        game.uma.vital,
                    )?;
                }
            }
            if !game.next() {
                break;
            }
        }
    }
    Ok(())
}
