from pathlib import Path

p = Path('crates/umasim/src/trainer/local_ramen_trainer.rs')
s = p.read_text(encoding='utf-8')

def one(old, new):
    global s
    if s.count(old) != 1:
        raise SystemExit(f'expected exactly one match, got {s.count(old)}: {old[:100]!r}')
    s = s.replace(old, new)

one(
'''    gamedata::{EventChoice, EventData, ramen::RAMENDATA}
};''',
'''    gamedata::{EventChoice, EventData, GAMECONSTANTS, ramen::RAMENDATA},
    global
};''')

one(
'''    pub status_reserve_max: f32,

    /// 是否使用随回合变化的体力成本模型。''',
'''    pub status_reserve_max: f32,

    /// 是否启用按五维完成度动态调整属性边际价值。
    ///
    /// 开启后会提高相对落后属性的精确评分边际，并在属性接近上限时降低继续堆叠的价值；
    /// 三张及以上同类型卡会放大对应属性的近上限衰减。默认关闭，仅供配对矩阵验证。
    pub dynamic_status_balance: bool,

    /// 短板追赶强度。1.0 表示完成度每落后最高维度 10%，该维精确属性评分边际增加 10%。
    pub status_gap_strength: f32,

    /// 近上限衰减强度。属性完成度超过 70% 后按平方曲线增长，并受同类型卡过量系数放大。
    pub status_overflow_strength: f32,

    /// 是否使用随回合变化的体力成本模型。''')

one(
'''            status_reserve_max: 0.,
            dynamic_vital: false,''',
'''            status_reserve_max: 0.,
            dynamic_status_balance: false,
            status_gap_strength: 0.0,
            status_overflow_strength: 0.0,
            dynamic_vital: false,''')

one(
'''            } else if token == "failmodel" {
                local.expected_fail = true''',
'''            } else if token == "statusdyn" {
                local.dynamic_status_balance = true
            } else if let Some(v) = token.strip_prefix("gap") {
                local.status_gap_strength = v.parse::<f32>()? / 100.0
            } else if let Some(v) = token.strip_prefix("over") {
                local.status_overflow_strength = v.parse::<f32>()? / 100.0
            } else if token == "failmodel" {
                local.expected_fail = true''')

one(
'''    fn vital_factor(t: i32) -> f32 {
        if t >= 72 { 0.25 } else { 3.5 + (t as f32 / 72.) * 2. }
    }
''',
'''    fn dynamic_status_adjustment(&self, g: &RamenGame, gain: &[i32; 6]) -> f32 {
        if !self.config.dynamic_status_balance {
            return 0.0;
        }
        let completion: [f32; 5] = std::array::from_fn(|i| {
            let limit = g.uma.five_status_limit[i].max(1) as f32;
            (g.uma.five_status[i].max(0) as f32 / limit).clamp(0.0, 1.0)
        });
        let leading = completion.iter().copied().fold(0.0f32, f32::max);
        let cons = global!(GAMECONSTANTS);
        let mut adjustment = 0.0;
        for i in 0..5 {
            let limit = g.uma.five_status_limit[i].max(0) as usize;
            let cur = (g.uma.five_status[i].max(0) as usize).min(limit);
            let next = cur.saturating_add(gain[i].max(0) as usize).min(limit);
            let cur_score = cons.five_status_final_score.get(cur).copied().unwrap_or(0) as f32;
            let next_score = cons.five_status_final_score.get(next).copied().unwrap_or(0) as f32;
            let exact_margin = (next_score - cur_score) * self.policy.config.status_rate;
            let gap_bonus = self.config.status_gap_strength * (leading - completion[i]).max(0.0);
            let near_cap = ((completion[i] - 0.70) / 0.30).clamp(0.0, 1.0);
            let excess_cards = (g.card_type_count[i] - 2).max(0) as f32;
            let overflow = self.config.status_overflow_strength
                * near_cap
                * near_cap
                * (1.0 + 0.5 * excess_cards);
            let multiplier = (1.0 + gap_bonus - overflow).clamp(0.10, 2.00);
            adjustment += exact_margin * (multiplier - 1.0);
        }
        adjustment
    }

    fn vital_factor(t: i32) -> f32 {
        if t >= 72 { 0.25 } else { 3.5 + (t as f32 / 72.) * 2. }
    }
''')

one(
'''            let rp = -self.reserve_penalty(g, &val.status_pt);
            o.score += rp;
            o.add("future_status_reserve", rp);
            if self.config.dynamic_vital {''',
'''            let rp = -self.reserve_penalty(g, &val.status_pt);
            o.score += rp;
            o.add("future_status_reserve", rp);
            let balance = self.dynamic_status_adjustment(g, &val.status_pt);
            o.score += balance;
            o.add("dynamic_status_balance", balance);
            if self.config.dynamic_vital {''')

p.write_text(s, encoding='utf-8')
