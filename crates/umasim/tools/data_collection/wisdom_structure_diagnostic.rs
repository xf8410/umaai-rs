//! 对 3速1耐1智候选逐局采集分年 PT、训练、Hint、吃面与恢复动作结构。

use std::{env, fs::File, io::Write, sync::Mutex};

use anyhow::{Context, Result};
use rand::prelude::StdRng;
use umasim::{
    bench::{self, CardPickOpts, DeckComposition},
    game::{
        Game, InheritInfo, Person, PersonType, Trainer,
        ramen::{Operation, RamenAction, RamenGame, RamenStage}
    },
    gamedata::{EventChoice, EventData, init_global_with_config},
    trainer::RecommendedRamenTrainer,
    utils::{get_workspace_root, load_game_config}
};

const BASE_SEED: u64 = 995_100;
const UMA: u32 = 102601;
const FRIEND: u32 = 303054;
const INHERIT: InheritInfo = InheritInfo {
    blue_count: [15, 0, 0, 0, 3],
    extra_count: [10, 10, 20, 20, 20, 40]
};

#[derive(Debug, Clone, Copy)]
struct SelectedTrain {
    operation: Operation,
    hint_cards: usize,
    shining: usize,
    ate_ramen: bool
}

struct DiagnosticTrainer {
    inner: RecommendedRamenTrainer,
    selected: Mutex<Option<SelectedTrain>>
}

impl DiagnosticTrainer {
    /// 创建候选策略的诊断包装器。
    fn new(inner: RecommendedRamenTrainer) -> Self {
        Self { inner, selected: Mutex::new(None) }
    }

    /// 取走本次 Train 阶段选择及其选择前盘面特征。
    fn take_selected(&self) -> Option<SelectedTrain> {
        self.selected.lock().ok()?.take()
    }
}

impl Trainer<RamenGame> for DiagnosticTrainer {
    fn select_action(&self, game: &RamenGame, actions: &[RamenAction], rng: &mut StdRng) -> Result<usize> {
        let index = self.inner.select_action(game, actions, rng)?;
        if game.stage == RamenStage::Train {
            let operation = actions.get(index).map(|action| action.operation).unwrap_or(Operation::Rest);
            let (hint_cards, shining) = if let Operation::Train(training) = operation {
                let train = training as usize;
                let hints = game
                    .distribution()
                    .get(train)
                    .into_iter()
                    .flatten()
                    .filter_map(|&person| usize::try_from(person).ok())
                    .filter(|&person| {
                        game.persons()
                            .get(person)
                            .is_some_and(|p| p.hint() && matches!(p.person_type(), PersonType::Card))
                    })
                    .count();
                (hints, game.shining_count(train))
            } else {
                (0, 0)
            };
            if let Ok(mut selected) = self.selected.lock() {
                *selected = Some(SelectedTrain {
                    operation,
                    hint_cards,
                    shining,
                    ate_ramen: game.ramen.current_ramen.is_some() || game.is_super_ramen_turn()
                });
            }
        }
        Ok(index)
    }

    fn select_choice(&self, game: &RamenGame, choices: &[Vec<EventChoice>], rng: &mut StdRng) -> Result<usize> {
        self.inner.select_choice(game, choices, rng)
    }

    fn select_event_choice(
        &self, game: &RamenGame, event: &EventData, choices: &[Vec<EventChoice>], rng: &mut StdRng
    ) -> Result<usize> {
        self.inner.select_event_choice(game, event, choices, rng)
    }
}

#[derive(Default)]
struct RunMetrics {
    year_skill_pt: [i32; 3],
    year_scenario_pt: [i32; 3],
    training: [usize; 5],
    shining_training: [usize; 5],
    hint_training: usize,
    hint_cards: usize,
    ramen_training: usize,
    rest: usize,
    friend_outing: usize,
    race: usize
}

/// 构建固定的 3速1耐1智代表卡组。
fn deck() -> Result<[u32; 6]> {
    let composition = DeckComposition {
        counts: [3, 1, 0, 0, 1],
        name: String::new()
    };
    let representatives = bench::select_representatives(&CardPickOpts::default())?;
    composition.build_deck(&representatives.picked, FRIEND)
}

/// 从环境变量构造均值或稳健候选。
fn candidate() -> Result<RecommendedRamenTrainer> {
    let variant = env::var("诊断方案")?;
    let (sacrifice, window, reserve, bond, hint) = match variant.as_str() {
        "均值候选" => (220.0, 0.15, 20.0, 12.0, 8.0),
        "稳健候选" => (180.0, 0.12, 40.0, 10.0, 7.0),
        _ => anyhow::bail!("未知诊断方案: {variant}")
    };
    Ok(RecommendedRamenTrainer::with_experiment_overrides(
        [32.0; 3], 0.75, 1.0, sacrifice, window, reserve, bond, hint
    ))
}

fn main() -> Result<()> {
    std::env::set_current_dir(get_workspace_root()?)?;
    init_global_with_config(&load_game_config()?)?;
    let variant = env::var("诊断方案")?;
    let shard: u64 = env::var("分片序号")?.parse()?;
    let runs: u64 = env::var("每分片局数")?.parse()?;
    let deck = deck()?;
    let mut file = File::create("3速1耐1智结构诊断.csv")?;
    writeln!(
        file,
        "方案,局序号,总分,技能点,第一年技能点,第二年技能点,第三年技能点,第一年拉面点,第二年拉面点,第三年拉面点,速度训练,耐力训练,力量训练,根性训练,智力训练,智力彩圈训练,Hint训练,Hint卡人次,吃面训练,休息,友人外出,比赛,RMJ成功年数,友人外出完成,速度,耐力,力量,根性,智力"
    )?;

    for offset in 0..runs {
        let run_index = shard * runs + offset;
        let (mut rng, rule_master) = bench::seeded_rngs(BASE_SEED, run_index);
        let mut game = RamenGame::newgame(UMA, &deck, INHERIT.clone())?;
        game.set_rule_master(rule_master);
        let trainer = DiagnosticTrainer::new(candidate()?);
        let mut metrics = RunMetrics::default();
        let mut previous_year_skill = 0;
        loop {
            let was_train = game.stage == RamenStage::Train;
            let turn = game.turn();
            game.run_stage(&trainer, &mut rng)
                .with_context(|| format!("方案={variant} 局={run_index} 回合={turn} 阶段={:?}", game.stage))?;
            if was_train {
                if let Some(selected) = trainer.take_selected() {
                    match selected.operation {
                        Operation::Train(training) => {
                            let train = training as usize;
                            metrics.training[train] += 1;
                            if selected.shining > 0 {
                                metrics.shining_training[train] += 1;
                            }
                            if selected.hint_cards > 0 {
                                metrics.hint_training += 1;
                                metrics.hint_cards += selected.hint_cards;
                            }
                            if selected.ate_ramen {
                                metrics.ramen_training += 1;
                            }
                        }
                        Operation::Rest => metrics.rest += 1,
                        Operation::FriendOuting => metrics.friend_outing += 1,
                        Operation::Race => metrics.race += 1,
                        _ => {}
                    }
                }
            }
            if matches!(turn, 23 | 47 | 71) && was_train {
                let year = if turn == 23 { 0 } else if turn == 47 { 1 } else { 2 };
                metrics.year_skill_pt[year] = game.uma.skill_pt - previous_year_skill;
                previous_year_skill = game.uma.skill_pt;
                metrics.year_scenario_pt[year] = game.ramen.scenario_pt;
            }
            if !game.next() {
                break;
            }
        }
        if metrics.year_skill_pt[2] == 0 {
            metrics.year_skill_pt[2] = game.uma.skill_pt - previous_year_skill;
        } else {
            metrics.year_skill_pt[2] += game.uma.skill_pt - previous_year_skill;
        }
        let status = game.uma.five_status;
        writeln!(
            file,
            "{variant},{run_index},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{}",
            game.uma.calc_score(), game.uma.skill_pt,
            metrics.year_skill_pt[0], metrics.year_skill_pt[1], metrics.year_skill_pt[2],
            metrics.year_scenario_pt[0], metrics.year_scenario_pt[1], metrics.year_scenario_pt[2],
            metrics.training[0], metrics.training[1], metrics.training[2], metrics.training[3], metrics.training[4],
            metrics.shining_training[4], metrics.hint_training, metrics.hint_cards, metrics.ramen_training,
            metrics.rest, metrics.friend_outing, metrics.race,
            game.ramen.rmj_results.iter().filter(|&&ok| ok).count(),
            game.friend.out_used.iter().all(|used| *used),
            status[0], status[1], status[2], status[3], status[4]
        )?;
    }
    Ok(())
}
