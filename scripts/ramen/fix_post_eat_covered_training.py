from pathlib import Path

p = Path('crates/umasim/src/trainer/local_ramen_trainer.rs')
s = p.read_text()
old = '''        if !self.friend_outing_within_pacing(g) && a.get(c).is_some_and(|x| x.operation == Operation::FriendOuting) {
            // 配额约束的是所有友人外出，而不只是“替代休息”路径。
            c = out
                .iter()
                .enumerate()
                .filter(|(i, _)| a.get(*i).is_some_and(|x| x.operation != Operation::FriendOuting))
                .max_by(|(li, l), (ri, r)| l.score.total_cmp(&r.score).then_with(|| ri.cmp(li)))
                .map(|(i, _)| i)
                .ok_or_else(|| anyhow::anyhow!("友人外出达到跨年总配额后没有其他合法动作"))?;
        }
        Ok((c, out))
'''
new = '''        if !self.friend_outing_within_pacing(g) && a.get(c).is_some_and(|x| x.operation == Operation::FriendOuting) {
            // 配额约束的是所有友人外出，而不只是“替代休息”路径。
            c = out
                .iter()
                .enumerate()
                .filter(|(i, _)| a.get(*i).is_some_and(|x| x.operation != Operation::FriendOuting))
                .max_by(|(li, l), (ri, r)| l.score.total_cmp(&r.score).then_with(|| ri.cmp(li)))
                .map(|(i, _)| i)
                .ok_or_else(|| anyhow::anyhow!("友人外出达到跨年总配额后没有其他合法动作"))?;
        }
        // 吃面是训练事务：不仅必须训练，还必须命中这碗面的定向训练范围。
        // 基础拉面效果虽对所有训练生效，但地区 xunlian/youqing/pt_bonus/hint 等价值
        // 只在 at_trains 内完整兑现。旧逻辑只排除休息/外出，导致约 24%–33% 的吃面
        // 回合随后点到未覆盖训练位，表现为“吃了 21 次、只加强约 15 次”。
        if self.config.eat_requires_training {
            if let Some(region_id) = g.ramen.current_ramen {
                let region = RAMENDATA
                    .get()
                    .and_then(|d| d.ramen_region_effect.get(region_id))
                    .ok_or_else(|| anyhow::anyhow!("吃面后缺少地区效果: {region_id}"))?;
                c = out
                    .iter()
                    .enumerate()
                    .filter(|(i, _)| {
                        a.get(*i).is_some_and(|x| match x.operation {
                            Operation::Train(t) => region.at_trains.contains(&(t as i32)),
                            _ => false
                        })
                    })
                    .max_by(|(li, l), (ri, r)| l.score.total_cmp(&r.score).then_with(|| ri.cmp(li)))
                    .map(|(i, _)| i)
                    .ok_or_else(|| anyhow::anyhow!("吃面 {region_id} 后没有覆盖训练候选"))?;
            }
        }
        Ok((c, out))
'''
if new in s:
    print('吃面后覆盖训练修复已存在')
elif s.count(old) != 1:
    raise SystemExit(f'目标片段匹配数量错误: {s.count(old)}')
else:
    p.write_text(s.replace(old, new))
    print('已写入吃面后覆盖训练修复')
