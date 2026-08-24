from pathlib import Path
import subprocess

subprocess.run(['python3', 'scripts/ramen/add_dynamic_status_balance_v39.py'], check=True)
p = Path('crates/umasim/src/trainer/local_ramen_trainer.rs')
s = p.read_text(encoding='utf-8')

def one(old, new):
    global s
    if s.count(old) != 1:
        raise SystemExit(f'expected one match, got {s.count(old)}: {old[:100]!r}')
    s = s.replace(old, new)

one(
'''    /// 是否启用按五维完成度动态调整属性边际价值。
    ///
    /// 开启后会提高相对落后属性的精确评分边际，并在属性接近上限时降低继续堆叠的价值；''',
'''    /// 五维精确属性评分边际的固定倍率 [速, 耐, 力, 根, 智]。
    ///
    /// 每份年度策略可独立配置；默认全 1.0。倍率只作用于对应属性的评分差分，
    /// 不会把该训练产生的 PT、副属性、羁绊和彩圈整体一起缩放。
    pub yearly_status_weights: [f32; 5],

    /// 是否启用按五维完成度动态调整属性边际价值。
    ///
    /// 开启后会提高相对落后属性的精确评分边际，并在属性接近上限时降低继续堆叠的价值；''')

one(
'''            status_reserve_max: 0.,
            dynamic_status_balance: false,''',
'''            status_reserve_max: 0.,
            yearly_status_weights: [1.0; 5],
            dynamic_status_balance: false,''')

one(
'''            } else if token == "statusdyn" {
                local.dynamic_status_balance = true''',
'''            } else if let Some(v) = token.strip_prefix("statusw") {
                let parts = v.split('_').map(str::parse::<f32>).collect::<std::result::Result<Vec<_>, _>>()?;
                if parts.len() != 5 {
                    anyhow::bail!("statusw 必须包含五个下划线分隔的百分数: {v}");
                }
                local.yearly_status_weights = std::array::from_fn(|i| parts[i] / 100.0);
            } else if token == "statusdyn" {
                local.dynamic_status_balance = true''')

one(
'''        if !self.config.dynamic_status_balance {
            return 0.0;
        }
        let completion: [f32; 5] = std::array::from_fn(|i| {''',
'''        let fixed_is_neutral = self.config.yearly_status_weights.iter().all(|&x| (x - 1.0).abs() < f32::EPSILON);
        if !self.config.dynamic_status_balance && fixed_is_neutral {
            return 0.0;
        }
        let completion: [f32; 5] = std::array::from_fn(|i| {''')

one(
'''            let gap_bonus = self.config.status_gap_strength * (leading - completion[i]).max(0.0);
            let near_cap = ((completion[i] - 0.70) / 0.30).clamp(0.0, 1.0);
            let excess_cards = (g.card_type_count[i] - 2).max(0) as f32;
            let overflow = self.config.status_overflow_strength
                * near_cap
                * near_cap
                * (1.0 + 0.5 * excess_cards);
            let multiplier = (1.0 + gap_bonus - overflow).clamp(0.10, 2.00);
            adjustment += exact_margin * (multiplier - 1.0);''',
'''            let (gap_bonus, overflow) = if self.config.dynamic_status_balance {
                let gap = self.config.status_gap_strength * (leading - completion[i]).max(0.0);
                let near_cap = ((completion[i] - 0.70) / 0.30).clamp(0.0, 1.0);
                let excess_cards = (g.card_type_count[i] - 2).max(0) as f32;
                let over = self.config.status_overflow_strength
                    * near_cap
                    * near_cap
                    * (1.0 + 0.5 * excess_cards);
                (gap, over)
            } else {
                (0.0, 0.0)
            };
            let dynamic = (1.0 + gap_bonus - overflow).clamp(0.10, 2.00);
            let multiplier = (self.config.yearly_status_weights[i] * dynamic).clamp(0.10, 2.00);
            adjustment += exact_margin * (multiplier - 1.0);''')

p.write_text(s, encoding='utf-8')
