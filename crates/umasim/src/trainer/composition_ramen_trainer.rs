//! 配卡画像感知的拉面策略。
//!
//! 三速一耐一智是一个明确的专用画像：第二、三年速度通常会自然成型，因此吃面窗口
//! 优先留给耐力/智力；只有诀窍库存临近溢出或速度出现双彩以上的大窗口时才为速度吃面。
//! 其他配卡不套用该规则，保持 v29 special-only 策略。

use std::sync::Mutex;

use anyhow::Result;
use rand::prelude::StdRng;

use crate::{
    game::{
        Game, PersonType, Trainer,
        ramen::{Operation, RamenAction, RamenGame, RamenStage},
    },
    gamedata::{EventChoice, EventData, ramen::RAMENDATA},
};

use super::LocalRamenTrainer;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RamenBuildProfile {
    ThreeSpeedOneStaminaOneWisdom,
    Other([usize; 5]),
}

impl RamenBuildProfile {
    pub fn detect(game: &RamenGame) -> Self {
        let mut counts = [0usize; 5];
        for card in game.deck() {
            let kind = card.data.card_type;
            if (0..5).contains(&kind) {
                counts[kind as usize] += 1;
            }
        }
        if counts == [3, 1, 0, 0, 1] {
            Self::ThreeSpeedOneStaminaOneWisdom
        } else {
            Self::Other(counts)
        }
    }
}

/// v29 `special-only` 基线。特意不启用退化的 deadline 例外。
pub struct V29SpecialTrainer {
    years: [LocalRamenTrainer; 3],
    last_year: Mutex<Option<usize>>,
}

impl V29SpecialTrainer {
    pub fn new() -> Result<Self> {
        fn make(pt: u32) -> Result<LocalRamenTrainer> {
            LocalRamenTrainer::matrix_variant(&format!(
                "pt{pt}-sac140-long-fail0-structall-rpt200-window10-look0-samples1-rawfail-cook240-vrest30-eatguard-friendrest-friendcap135-friendspecial4-specialdynamic"
            ))
        }
        Ok(Self {
            years: [make(16)?, make(64)?, make(64)?],
            last_year: Mutex::new(None),
        })
    }

    fn year(game: &RamenGame) -> usize {
        if game.turn() < 24 { 0 } else if game.turn() < 48 { 1 } else { 2 }
    }
}

impl Trainer<RamenGame> for V29SpecialTrainer {
    fn select_action(&self, game: &RamenGame, actions: &[RamenAction], rng: &mut StdRng) -> Result<usize> {
        let year = Self::year(game);
        *self.last_year.lock().unwrap() = Some(year);
        self.years[year].select_action(game, actions, rng)
    }

    fn select_choice(&self, game: &RamenGame, choices: &[Vec<EventChoice>], rng: &mut StdRng) -> Result<usize> {
        let year = Self::year(game);
        *self.last_year.lock().unwrap() = Some(year);
        self.years[year].select_choice(game, choices, rng)
    }

    fn select_event_choice(
        &self,
        game: &RamenGame,
        event: &EventData,
        choices: &[Vec<EventChoice>],
        rng: &mut StdRng,
    ) -> Result<usize> {
        let year = Self::year(game);
        *self.last_year.lock().unwrap() = Some(year);
        self.years[year].select_event_choice(game, event, choices, rng)
    }

    fn last_breakdown(&self) -> Option<String> {
        let year = (*self.last_year.lock().ok()?)?;
        self.years[year].last_breakdown()
    }
}

/// 在 v29-special 上增加三速卡组的第二、三年吃面窗口规则。
pub struct CompositionRamenTrainer {
    base: V29SpecialTrainer,
    last_override: Mutex<Option<String>>,
}

impl CompositionRamenTrainer {
    pub fn new() -> Result<Self> {
        Ok(Self {
            base: V29SpecialTrainer::new()?,
            last_override: Mutex::new(None),
        })
    }

    fn people_and_shining(game: &RamenGame, train: usize) -> (usize, usize) {
        let people = game
            .distribution()
            .get(train)
            .into_iter()
            .flatten()
            .filter(|&&id| id >= 0 && (id as usize) < game.persons().len())
            .filter(|&&id| {
                matches!(
                    game.persons()[id as usize].person_type,
                    PersonType::Card | PersonType::ScenarioCard
                )
            })
            .count();
        (people, game.shining_count(train))
    }

    /// 返回三速画像本回合值得为其吃面的训练位置和窗口强度。
    fn three_speed_target(game: &RamenGame) -> Option<(usize, i32, &'static str)> {
        if game.current_year() < 2 || game.turn() >= 72 {
            return None;
        }
        let (sta_people, sta_shining) = Self::people_and_shining(game, 1);
        let (wis_people, wis_shining) = Self::people_and_shining(game, 4);
        let (spd_people, spd_shining) = Self::people_and_shining(game, 0);

        // 三速时耐力/智力只要有两个有效人头就形成吃面窗口；彩圈是额外优先级。
        let stamina = (sta_people >= 2).then_some((1, 100 + sta_people as i32 * 12 + sta_shining as i32 * 45, "耐力两人窗口"));
        let wisdom = (wis_people >= 2).then_some((4, 100 + wis_people as i32 * 12 + wis_shining as i32 * 45, "智力两人窗口"));

        // 速度通常忽略。只有诀窍库存已经临近溢出，或速度双彩以上，才开放速度窗口。
        let stock_overflow = game.ramen.feeling_stock.iter().sum::<i32>() >= 8;
        let speed_exception = stock_overflow || spd_shining >= 2;
        let speed = speed_exception.then_some((
            0,
            40 + spd_people as i32 * 8 + spd_shining as i32 * 55 + if stock_overflow { 70 } else { 0 },
            if stock_overflow { "速度库存溢出例外" } else { "速度双彩大窗口例外" },
        ));

        [stamina, wisdom, speed].into_iter().flatten().max_by_key(|x| x.1)
    }

    fn override_ramen(&self, game: &RamenGame, actions: &[RamenAction]) -> Option<(usize, String)> {
        if game.stage != RamenStage::RamenSelect
            || RamenBuildProfile::detect(game) != RamenBuildProfile::ThreeSpeedOneStaminaOneWisdom
        {
            return None;
        }
        let (target, window, reason) = Self::three_speed_target(game)?;
        let data = RAMENDATA.get()?;
        actions
            .iter()
            .enumerate()
            .filter_map(|(index, action)| {
                let region_id = action.ramen?;
                let region = data.ramen_region_effect.get(region_id)?;
                if !region.at_trains.contains(&(target as i32)) {
                    return None;
                }
                let effect = region.xunlian * 4 + region.youqing * 3 + region.pt_bonus * 2 + region.hint_count * 20;
                Some((index, effect))
            })
            .max_by_key(|(_, effect)| *effect)
            .map(|(index, effect)| {
                (
                    index,
                    format!("三速画像: {reason}; window={window}, region_effect={effect}"),
                )
            })
    }
}

impl Trainer<RamenGame> for CompositionRamenTrainer {
    fn select_action(&self, game: &RamenGame, actions: &[RamenAction], rng: &mut StdRng) -> Result<usize> {
        if let Some((index, reason)) = self.override_ramen(game, actions) {
            *self.last_override.lock().unwrap() = Some(reason);
            return Ok(index);
        }
        *self.last_override.lock().unwrap() = None;
        self.base.select_action(game, actions, rng)
    }

    fn select_choice(&self, game: &RamenGame, choices: &[Vec<EventChoice>], rng: &mut StdRng) -> Result<usize> {
        self.base.select_choice(game, choices, rng)
    }

    fn select_event_choice(
        &self,
        game: &RamenGame,
        event: &EventData,
        choices: &[Vec<EventChoice>],
        rng: &mut StdRng,
    ) -> Result<usize> {
        self.base.select_event_choice(game, event, choices, rng)
    }

    fn last_breakdown(&self) -> Option<String> {
        self.last_override.lock().ok().and_then(|x| x.clone()).or_else(|| self.base.last_breakdown())
    }
}
