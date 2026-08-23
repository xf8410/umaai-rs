from pathlib import Path
p=Path('crates/umasim/src/trainer/local_ramen_trainer.rs')
s=p.read_text()

anchor='''    pub friend_outing3_recovery_vital: i32,
}'''
replace='''    pub friend_outing3_recovery_vital: i32,

    /// 各年结束前允许累计使用的友人外出次数上限。
    ///
    /// 五次外出是整局有限资源，每次还产生 2 个万能材料；不能因为第一年休息较多就一次用完。
    /// 例如 `[1, 3, 5]` 表示第一年最多用 1 次、第二年结束前最多累计 3 次、第三年可用完。
    /// `[5, 5, 5]` 等价于不做跨年配额；仅在 `friend_outing_replaces_rest=true` 时生效。
    pub friend_outing_cumulative_caps: [usize; 3],
}'''
if s.count(anchor)!=1: raise SystemExit(f'cfg {s.count(anchor)}')
s=s.replace(anchor,replace)
s=s.replace('''            friend_outing3_recovery_vital: 0,
''','''            friend_outing3_recovery_vital: 0,
            friend_outing_cumulative_caps: [5, 5, 5],
''',1)

anchor='''            } else if let Some(v) = token.strip_prefix("friend3v") {
                local.friend_outing3_recovery_vital = v.parse()?
            } else if token == "failmodel" {
'''
replace='''            } else if let Some(v) = token.strip_prefix("friend3v") {
                local.friend_outing3_recovery_vital = v.parse()?
            } else if let Some(v) = token.strip_prefix("friendcap") {
                let digits = v.as_bytes();
                if digits.len() != 3 || !digits.iter().all(u8::is_ascii_digit) {
                    anyhow::bail!("friendcap 必须是三个数字，如 135: {v}");
                }
                local.friend_outing_cumulative_caps = [
                    (digits[0] - b'0') as usize,
                    (digits[1] - b'0') as usize,
                    (digits[2] - b'0') as usize,
                ];
                let c = local.friend_outing_cumulative_caps;
                if c[0] > c[1] || c[1] > c[2] || c[2] > 5 {
                    anyhow::bail!("friendcap 必须单调且不超过5: {v}");
                }
            } else if token == "failmodel" {
'''
if s.count(anchor)!=1: raise SystemExit(f'parser {s.count(anchor)}')
s=s.replace(anchor,replace)

# Add helper before decide_train.
anchor='''    fn decide_train(&self, g: &RamenGame, a: &[RamenAction]) -> Result<(usize, Vec<RamenPolicyOutput>)> {
'''
helper='''    /// 本年是否仍有友人外出配额。配额按整局累计次数控制，而不是每年重置。
    fn friend_outing_within_pacing(&self, g: &RamenGame) -> bool {
        let year = (g.current_year() - 1).clamp(0, 2) as usize;
        let used = g.friend.out_used.iter().filter(|&&x| x).count();
        used < self.config.friend_outing_cumulative_caps[year]
    }

'''
if s.count(anchor)!=1: raise SystemExit(f'helper anchor {s.count(anchor)}')
s=s.replace(anchor,helper+anchor)

# Require pacing in replacement condition.
old='''        if self.config.friend_outing_replaces_rest
            && a.get(guard).is_some_and(|x| x.operation == Operation::Rest)
'''
new='''        if self.config.friend_outing_replaces_rest
            && self.friend_outing_within_pacing(g)
            && a.get(guard).is_some_and(|x| x.operation == Operation::Rest)
'''
if s.count(old)!=1: raise SystemExit(f'condition {s.count(old)}')
s=s.replace(old,new)

# Keep formal disabled pending paced benchmark, but document finite resource rule.
s=s.replace('''/// - 友人外出沿用既有候选打分与事件逻辑；“替代休息”仅保留为矩阵实验，尚未进入正式 preset。
''','''/// - 友人外出沿用既有候选打分；“替代休息”仅作矩阵实验，并以跨年累计配额防止第一年耗尽五次万能材料来源。
''',1)
p.write_text(s)
