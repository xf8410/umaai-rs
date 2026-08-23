from pathlib import Path
p=Path('crates/umasim/src/trainer/local_ramen_trainer.rs')
s=p.read_text()

anchor='''    pub friend_outing_cumulative_caps: [usize; 3],
}'''
replace='''    pub friend_outing_cumulative_caps: [usize; 3],

    /// “休息→友人外出”替代时允许的最高当前万能材料数量。
    ///
    /// 外出固定获得 2 个万能材料且上限为 4；设为 2 可避免替代路径产生材料溢出。
    /// 原策略主动选择友人外出不受此门控，只受总次数配额约束。`4` 表示关闭。
    pub friend_rest_max_special: i32,
}'''
if s.count(anchor)!=1: raise SystemExit(f'cfg anchor {s.count(anchor)}')
s=s.replace(anchor,replace)
s=s.replace('''            friend_outing_cumulative_caps: [5, 5, 5],
''','''            friend_outing_cumulative_caps: [5, 5, 5],
            friend_rest_max_special: 4,
''',1)

anchor='''            } else if token == "failmodel" {
'''
replace='''            } else if let Some(v) = token.strip_prefix("friendspecial") {
                local.friend_rest_max_special = v.parse()?
            } else if token == "failmodel" {
'''
if s.count(anchor)!=1: raise SystemExit(f'parser anchor {s.count(anchor)}')
s=s.replace(anchor,replace,1)

# Replacement path: total cap plus material overflow check.
old='''        if self.config.friend_outing_replaces_rest
            && self.friend_outing_within_pacing(g)
            && a.get(guard).is_some_and(|x| x.operation == Operation::Rest)
'''
new='''        if self.config.friend_outing_replaces_rest
            && self.friend_outing_within_pacing(g)
            && g.ramen.special_feeling <= self.config.friend_rest_max_special
            && a.get(guard).is_some_and(|x| x.operation == Operation::Rest)
'''
if s.count(old)!=1: raise SystemExit(f'replace gate {s.count(old)}')
s=s.replace(old,new)

# Enforce true total cap after all local scoring/sacrifice logic. This catches ordinary policy
# FriendOuting selection as well as replacement path.
old='''        let c = if sacrifice <= self.config.max_base_score_sacrifice {
            lb
        } else {
            bb
        };
        Ok((c, out))
'''
new='''        let mut c = if sacrifice <= self.config.max_base_score_sacrifice {
            lb
        } else {
            bb
        };
        if !self.friend_outing_within_pacing(g)
            && a.get(c).is_some_and(|x| x.operation == Operation::FriendOuting)
        {
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
if s.count(old)!=1: raise SystemExit(f'final choice {s.count(old)}')
s=s.replace(old,new,1)

# Keep production disabled until paired validation.
p.write_text(s)
