from pathlib import Path
import os

threshold = float(os.environ.get('POST_EAT_SOFT_THRESHOLD', '150'))
p = Path('crates/umasim/src/trainer/local_ramen_trainer.rs')
s = p.read_text()
marker = '        Ok((c, out))\n    }\n    fn pt_effect('
insert = f'''        // 实验：吃面后软优先该面覆盖训练。覆盖内最佳相对全局最佳损失不超过
        // {threshold:.1f} 策略分时切换；否则保留明显更强的非覆盖训练窗口。
        if self.config.eat_requires_training {{
            if let Some(region_id) = g.ramen.current_ramen {{
                let region = RAMENDATA
                    .get()
                    .and_then(|d| d.ramen_region_effect.get(region_id))
                    .ok_or_else(|| anyhow::anyhow!("吃面后缺少地区效果: {{region_id}}"))?;
                let covered = out
                    .iter()
                    .enumerate()
                    .filter(|(i, _)| {{
                        a.get(*i).is_some_and(|x| match x.operation {{
                            Operation::Train(t) => region.at_trains.contains(&(t as i32)),
                            _ => false
                        }})
                    }})
                    .max_by(|(li, l), (ri, r)| l.score.total_cmp(&r.score).then_with(|| ri.cmp(li)))
                    .map(|(i, o)| (i, o.score));
                if let Some((covered_idx, covered_score)) = covered {{
                    let chosen_score = out.get(c).map(|o| o.score).unwrap_or(f32::NEG_INFINITY);
                    if chosen_score - covered_score <= {threshold:.6} {{
                        c = covered_idx;
                    }}
                }}
            }}
        }}
        Ok((c, out))
    }}
    fn pt_effect('''
if insert in s:
    print('软门控已存在')
elif s.count(marker) != 1:
    raise SystemExit(f'目标标记匹配数量错误: {s.count(marker)}')
else:
    p.write_text(s.replace(marker, insert))
    print(f'已加入吃后软门控，阈值={threshold}')
