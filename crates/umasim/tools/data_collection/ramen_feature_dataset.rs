//! 拉面杯特征矩阵教师数据导出。
//!
//! 每个 index 确定性生成一个五配卡根局面，以 FlatSearch 的完整终局 rollout 均值作为
//! 候选动作标签，并同时导出 S/M/M-ID/L 四套版本化特征。

use std::{env, fs::File, io::Write};

use anyhow::Result;
use serde::Serialize;
use umasim::{
    game::{
        Game,
        InheritInfo,
        Person,
        ramen::{Operation, RamenAction, RamenGame, RamenStage}
    },
    gamedata::{GAMECONSTANTS, init_global_with_config, ramen::RAMENDATA},
    global,
    sampler::{SampleOutcome, SampleSpec, sample_from_spec},
    search::{FlatSearch, SearchConfig},
    utils::{get_workspace_root, load_game_config}
};

const UMA: u32 = 102601;
const BASE_SEED: u64 = 0x5241_4d45_4e5f_4e4e;
const DECKS: [[u32; 6]; 5] = [
    [303024, 302984, 302924, 303074, 303094, 303054],
    [303024, 302984, 303074, 303044, 303094, 303054],
    [303014, 302974, 303094, 303064, 302894, 303054],
    [303024, 302984, 303074, 303094, 303064, 303054],
    [303024, 303074, 303094, 303064, 302894, 303054]
];
const BUILD_NAMES: [&str; 5] = ["3速1耐1智", "2速2耐1智", "2力3智", "2速1耐2智", "1速1耐3智"];
const INHERIT: InheritInfo = InheritInfo {
    blue_count: [15, 0, 0, 0, 3],
    extra_count: [10, 10, 20, 20, 20, 40]
};

#[derive(Serialize)]
struct Row {
    group: u64,
    build: &'static str,
    turn: i32,
    stage: String,
    action_index: usize,
    action: String,
    teacher_mean: f32,
    teacher_stdev: f32,
    s: Vec<f32>,
    m: Vec<f32>,
    mid: Vec<f32>,
    l: Vec<f32>
}

fn stage_index(stage: &RamenStage) -> usize {
    match stage {
        RamenStage::Begin => 0,
        RamenStage::Distribute => 1,
        RamenStage::RamenSelect => 2,
        RamenStage::SpecialSelect => 3,
        RamenStage::Train => 4,
        RamenStage::AfterTrain => 5,
        RamenStage::NextTurn => 6,
        RamenStage::RegionSelect => 7,
        RamenStage::SuperRamenSelect => 8,
        RamenStage::Settlement => 9,
        RamenStage::BeginAfterRegionSelect => 10
    }
}

fn action_features(game: &RamenGame, action: &RamenAction) -> Vec<f32> {
    let mut f = vec![0.0; 58];
    match action.operation {
        Operation::Train(t) => f[t as usize] = 1.0,
        Operation::Race => f[5] = 1.0,
        Operation::Rest => f[6] = 1.0,
        Operation::NormalOuting => f[7] = 1.0,
        Operation::FriendOuting => f[8] = 1.0,
        Operation::Clinic => f[9] = 1.0,
        Operation::RegionSelect(regions) => {
            f[10] = 1.0;
            for r in regions {
                if r < 20 {
                    f[31 + r] = 1.0;
                }
            }
        }
        Operation::StageOnly => f[11] = 1.0,
        // FIXME: 可能和其他特征冲突。临时值
        Operation::SuperRamenSelect(sup) => f[13] = sup as f32
    }
    if let Some(r) = action.ramen {
        f[12] = 1.0;
        if r < 20 {
            f[13 + r.min(17)] = 1.0;
        }
        if let Some(region) = RAMENDATA.get().and_then(|d| d.ramen_region_effect.get(r)) {
            f[51] = region.xunlian as f32 / 100.0;
            f[52] = region.youqing as f32 / 100.0;
            f[53] = region.pt_bonus as f32 / 100.0;
            f[54] = region.hint_count as f32 / 5.0;
        }
    }
    if let Some(t) = action.special_targets {
        for i in 0..3 {
            f[55 + i] = t[i] as f32 / 4.0;
        }
    }
    let _ = game;
    f
}

fn state_s(game: &RamenGame) -> Result<Vec<f32>> {
    let mut f = Vec::new();
    f.push(game.turn() as f32 / 77.0);
    f.push(game.current_year() as f32 / 3.0);
    for i in 0..10 {
        f.push((stage_index(&game.stage) == i) as u8 as f32);
    }
    for &v in &game.uma.five_status {
        f.push(v as f32 / 3000.0);
    }
    for &v in &game.uma.five_status_limit {
        f.push(v as f32 / 3000.0);
    }
    for i in 0..5 {
        f.push((game.uma.five_status_limit[i] - game.uma.five_status[i]).max(0) as f32 / 3000.0);
    }
    f.extend([
        game.uma.vital as f32 / 100.0,
        game.uma.max_vital as f32 / 120.0,
        game.uma.motivation as f32 / 5.0,
        game.uma.skill_pt as f32 / 15000.0,
        game.uma.total_hints as f32 / 100.0,
        game.ramen.scenario_pt as f32 / 6000.0,
        game.ramen.special_feeling as f32 / 4.0,
        game.ramen.eat_count as f32 / 8.0,
        game.ramen.train_level_bonus as f32 / 5.0,
        game.ramen.rmj_results.len() as f32 / 3.0
    ]);
    for &v in &game.ramen.feeling_stock {
        f.push(v as f32 / 10.0);
    }
    for &v in &game.ramen.feeling_slot {
        f.push(v as f32 / 7.0);
    }
    for &v in &game.card_type_count[..5] {
        f.push(v as f32 / 3.0);
    }
    for train in 0..5 {
        let buffs = game.calc_training_buff(train)?;
        let value = game.calc_training_value(&buffs, train)?;
        for &v in &value.status_pt {
            f.push(v as f32 / 200.0);
        }
        f.push(value.vital as f32 / 100.0);
        f.push(game.calc_training_failure_rate(&buffs, train) / 100.0);
        f.push(game.distribution().get(train).map(Vec::len).unwrap_or(0) as f32 / 5.0);
        f.push(game.shining_count(train) as f32 / 5.0);
        let hints = game
            .distribution()
            .get(train)
            .into_iter()
            .flatten()
            .filter(|&&p| p >= 0 && game.persons()[p as usize].hint())
            .count();
        f.push(hints as f32 / 5.0);
    }
    Ok(f)
}

fn state_m(game: &RamenGame, mut f: Vec<f32>, with_id: bool) -> Vec<f32> {
    for (idx, card) in game.deck().iter().enumerate() {
        for t in 0..6 {
            f.push((card.card_type.clamp(0, 5) as usize == t) as u8 as f32);
        }
        f.push(card.friendship as f32 / 100.0);
        f.push(card.rank as f32 / 4.0);
        f.push(card.total_hints as f32 / 20.0);
        f.push(card.effect.youqing / 100.0);
        f.push(card.effect.xunlian as f32 / 100.0);
        f.push(card.effect.ganjing as f32 / 100.0);
        f.push(card.effect.deyilv / 200.0);
        f.push(card.effect.wiz_vital_bonus as f32 / 20.0);
        f.push(card.effect.hint_count_bonus as f32 / 5.0);
        for train in 0..5 {
            f.push(
                game.distribution()
                    .get(train)
                    .is_some_and(|d| d.contains(&(idx as i32))) as u8 as f32
            );
        }
        if with_id {
            f.push(card.card_id as f32 / 40000.0);
            f.push(card.data.chara_id as f32 / 2000.0);
        }
    }
    f
}

fn state_l(game: &RamenGame, mut f: Vec<f32>) -> Vec<f32> {
    for train in 0..5 {
        for slot in 0..5 {
            let person = game.distribution().get(train).and_then(|d| d.get(slot)).copied();
            f.push(person.is_some() as u8 as f32);
            f.push(
                person
                    .filter(|p| *p >= 0)
                    .map(|p| game.persons()[p as usize].friendship() as f32 / 100.0)
                    .unwrap_or(0.0)
            );
            f.push(
                person
                    .filter(|p| *p >= 0)
                    .map(|p| game.persons()[p as usize].hint() as u8 as f32)
                    .unwrap_or(0.0)
            );
        }
    }
    f.extend([
        game.current_effect.xunlian as f32 / 100.0,
        game.current_effect.youqing as f32 / 100.0,
        game.current_effect.pt_bonus as f32 / 100.0,
        game.current_effect.fail_rate_drop / 100.0,
        game.current_effect.friendship as f32 / 20.0,
        game.current_effect.deyilv as f32 / 200.0,
        game.current_effect.hint as f32 / 100.0,
        game.current_effect.clone as f32 / 6.0,
        game.current_effect.hint_special as u8 as f32,
        game.deck_can_split as u8 as f32
    ]);
    for r in 0..20 {
        f.push(game.ramen.selected_regions.contains(&r) as u8 as f32);
    }
    f
}

fn main() -> Result<()> {
    std::env::set_current_dir(get_workspace_root()?)?;
    init_global_with_config(&load_game_config()?)?;
    let start: u64 = env::var("START").unwrap_or_else(|_| "0".into()).parse()?;
    let count: u64 = env::var("COUNT").unwrap_or_else(|_| "100".into()).parse()?;
    let rollouts: usize = env::var("ROLLOUTS").unwrap_or_else(|_| "32".into()).parse()?;
    let search: FlatSearch<RamenGame> = FlatSearch::new(
        SearchConfig::default()
            .with_search_n(rollouts)
            .with_ucb(false)
            .with_radical_factor_max(0.0)
    );
    let mut out = File::create("ramen-feature-dataset.jsonl")?;
    for index in start..start + count {
        let deck_idx = index as usize % DECKS.len();
        let spec = SampleSpec {
            index,
            uma: UMA,
            deck: DECKS[deck_idx],
            shape: BUILD_NAMES[deck_idx],
            inherit: INHERIT,
            truncate_turn: ((index.wrapping_mul(37).wrapping_add(11)) % 72) as i32,
            seed: BASE_SEED ^ index.wrapping_mul(0x9e37_79b9_7f4a_7c15),
            epsilon: 0.15,
            min_actions: 2
        };
        let SampleOutcome::Captured(pos) = sample_from_spec(spec)? else {
            continue;
        };
        let mut rng = pos.decision_rng.clone();
        let result = search.search(&pos.game, &pos.actions, &mut rng)?;
        let s0 = state_s(&pos.game)?;
        let m0 = state_m(&pos.game, s0.clone(), false);
        let mid0 = state_m(&pos.game, s0.clone(), true);
        let l0 = state_l(&pos.game, mid0.clone());
        for (action_index, action) in pos.actions.iter().enumerate() {
            let af = action_features(&pos.game, action);
            let append = |mut x: Vec<f32>| {
                x.extend_from_slice(&af);
                x
            };
            let ar = &result.action_results[action_index].0;
            let row = Row {
                group: index,
                build: BUILD_NAMES[deck_idx],
                turn: pos.turn,
                stage: format!("{:?}", pos.stage),
                action_index,
                action: action.to_string(),
                teacher_mean: ar.mean() as f32,
                teacher_stdev: ar.stdev() as f32,
                s: append(s0.clone()),
                m: append(m0.clone()),
                mid: append(mid0.clone()),
                l: append(l0.clone())
            };
            serde_json::to_writer(&mut out, &row)?;
            writeln!(out)?;
        }
    }
    eprintln!(
        "schema dims: S={} M={} MID={} L={} (含动作58维)",
        state_s(&RamenGame::newgame(UMA, &DECKS[0], INHERIT)?)?.len() + 58,
        0,
        0,
        0
    );
    let _ = global!(GAMECONSTANTS);
    Ok(())
}
