from pathlib import Path
p=Path('crates/umasim/src/trainer/local_ramen_trainer.rs')
s=p.read_text()

anchor='''    pub eat_requires_training: bool,
}'''
replace='''    pub eat_requires_training: bool,

    /// 第三年吃面前要求“吃面后所选训练”结束时至少保留的体力。
    ///
    /// 仅在普通第三年回合（48-71）生效。策略会针对每碗候选面预演吃面后的最佳训练；
    /// 若非智力训练结束后的体力低于该值，就禁止该面，避免虽然本回合因 100% 减失败率
    /// 能训练，却把体力打空并导致下一回合被迫恢复。智力训练会回体力，不受此门控。
    /// `0` 表示关闭。
    pub y3_post_train_vital_floor: i32,
}'''
if s.count(anchor)!=1: raise SystemExit(f'config anchor {s.count(anchor)}')
s=s.replace(anchor,replace)
s=s.replace('''            eat_requires_training: false,
''','''            eat_requires_training: false,
            y3_post_train_vital_floor: 0,
''',1)

anchor='''            } else if token == "eatguard" {
                local.eat_requires_training = true
            } else if token == "failmodel" {
'''
replace='''            } else if token == "eatguard" {
                local.eat_requires_training = true
            } else if let Some(v) = token.strip_prefix("y3floor") {
                local.y3_post_train_vital_floor = v.parse()?
            } else if token == "failmodel" {
'''
if s.count(anchor)!=1: raise SystemExit(f'parser anchor {s.count(anchor)}')
s=s.replace(anchor,replace)
s=s.replace('''            local.eat_requires_training = true;
''','''            local.eat_requires_training = true;
            local.y3_post_train_vital_floor = 20;
''',1)
s=s.replace('''/// - 吃面前先决定是否训练；吃面后强制从训练候选中选择，禁止休息浪费加成。
''','''/// - 吃面前先决定是否训练；吃面后强制从训练候选中选择，禁止休息浪费加成；
/// - 第三年逐碗预演吃面后的训练，非智力训练后至少保留 20 体力，保护下一回合。
''',1)

# Candidate preview helper. It uses actual post-eat training values but does not consume resources or RNG.
anchor='''    fn best_action_score(&self, g: &RamenGame) -> Result<f32> {
'''
helper='''    /// 预演第三年某碗面落地后的最佳训练，返回 `(训练类型, 训练后体力)`。
    ///
    /// 不落地随机分身，因此只使用当前已知人头与确定性拉面效果；这是保下限门控，
    /// 不是用于给候选增加收益的随机 lookahead。
    fn post_ramen_training_vital(&self, g: &RamenGame, region_id: usize) -> Result<Option<(usize, i32)>> {
        if self.config.y3_post_train_vital_floor <= 0 || g.current_year() != 3 || g.turn() >= 72 {
            return Ok(None);
        }
        let mut preview = g.clone();
        preview.stage = RamenStage::Train;
        preview.ramen.current_ramen = Some(region_id);
        preview.ramen.clear_pending();
        let actions = preview.list_actions()?;
        let (idx, _) = self.decide_train(&preview, &actions)?;
        let Some(action) = actions.get(idx) else {
            anyhow::bail!("吃面后预演索引越界: {idx}/{}", actions.len());
        };
        let Operation::Train(tt) = action.operation else {
            return Ok(None);
        };
        let train = tt as usize;
        let buffs = preview.calc_training_buff(train)?;
        let value = preview.calc_training_value(&buffs, train)?;
        Ok(Some((train, preview.uma.vital + value.vital)))
    }

'''
if s.count(anchor)!=1: raise SystemExit(f'helper anchor {s.count(anchor)}')
s=s.replace(anchor,helper+anchor)

# Candidate-specific rejection before ordinary ramen bonuses are applied.
anchor='''            if let Some(region_id) = act.ramen {
                let pressure = risk * self.config.overflow_value;
'''
replace='''            if let Some(region_id) = act.ramen {
                if let Some((train, post_vital)) = self.post_ramen_training_vital(g, region_id)? {
                    let wisdom_recovers = train == 4;
                    if !wisdom_recovers && post_vital < self.config.y3_post_train_vital_floor {
                        o.score = f32::NEG_INFINITY;
                        o.reason = format!(
                            "禁止吃面：第三年预演{}训练后体力{}<保留{}",
                            ["速", "耐", "力", "根", "智"][train],
                            post_vital,
                            self.config.y3_post_train_vital_floor
                        );
                        o.add("y3_next_turn_vital_guard", f32::NEG_INFINITY);
                        continue;
                    }
                }
                let pressure = risk * self.config.overflow_value;
'''
if s.count(anchor)!=1: raise SystemExit(f'candidate anchor {s.count(anchor)}')
s=s.replace(anchor,replace)

p.write_text(s)
