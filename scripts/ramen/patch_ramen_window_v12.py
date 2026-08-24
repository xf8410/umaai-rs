from pathlib import Path
p=Path('crates/umasim/src/trainer/local_ramen_trainer.rs')
s=p.read_text()
s=s.replace('''    pub eager_eat: bool,
''','''    pub eager_eat: bool,
    /// v8 诊断：吃面前，候选地区覆盖的当前训练窗口价值。
    pub ramen_window_weight: f32,
''')
s=s.replace('''            eager_eat: false,
''','''            eager_eat: false,
            ramen_window_weight: 0.0,
''')
s=s.replace('''            } else if let Some(v) = token.strip_prefix("look") {
''','''            } else if let Some(v) = token.strip_prefix("window") {
                local.ramen_window_weight = v.parse::<f32>()? / 100.0
            } else if let Some(v) = token.strip_prefix("look") {
''')
anchor='''    /// 在真正吃面前，用状态副本执行候选面并评估其事后最佳动作。
'''
method='''    /// 精确复原 v8 的吃面前窗口信号，用于解释其收益来源。
    /// 它只查看候选地区 at_trains 当前已有的真实训练窗口，不预测分身。
    fn ramen_window_alignment(&self, g: &RamenGame, region_id: usize) -> Result<f32> {
        if self.config.ramen_window_weight <= 0.0 {
            return Ok(0.0);
        }
        let d = RAMENDATA.get().ok_or_else(|| anyhow::anyhow!("RAMENDATA 未初始化"))?;
        let region = d.ramen_region_effect.get(region_id)
            .ok_or_else(|| anyhow::anyhow!("地区效果缺失: {region_id}"))?;
        let mut best = 0.0f32;
        for &t in &region.at_trains {
            if !(0..5).contains(&t) { continue; }
            let tr = t as usize;
            let buffs = g.calc_training_buff(tr)?;
            let v = g.calc_training_value(&buffs, tr)?;
            let raw = v.status_pt[..5].iter().sum::<i32>() as f32 + v.status_pt[5] as f32 * 2.0;
            let people = g.distribution().get(tr).map(|x| x.len()).unwrap_or(0) as f32;
            let shining = g.shining_count(tr) as f32;
            best = best.max(raw + people * 8.0 + shining * 35.0);
        }
        let effect = (region.xunlian + region.youqing + region.pt_bonus) as f32
            + region.hint_count as f32 * 10.0;
        Ok(best * effect * self.config.ramen_window_weight / 100.0)
    }
'''
if anchor not in s: raise SystemExit('lookahead anchor missing')
s=s.replace(anchor,method+anchor)
old='''                let look = self.ramen_lookahead(g, region_id)?;
                o.score += ck + rmj + great + look;
'''
new='''                let window = self.ramen_window_alignment(g, region_id)?;
                let look = self.ramen_lookahead(g, region_id)?;
                o.score += ck + rmj + great + window + look;
'''
if old not in s: raise SystemExit('score block missing')
s=s.replace(old,new)
s=s.replace('''                o.add("ramen_lookahead", look)
''','''                o.add("ramen_window", window);
                o.add("ramen_lookahead", look)
''')
p.write_text(s)
