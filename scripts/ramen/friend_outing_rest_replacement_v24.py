from pathlib import Path
p=Path('crates/umasim/src/trainer/local_ramen_trainer.rs')
s=p.read_text()

anchor='''    pub y3_recovery_horizon: bool,
}'''
replace='''    pub y3_recovery_horizon: bool,

    /// 当体力守门或正常打分原本选择休息时，是否优先用尚未完成的友人外出替代。
    ///
    /// 友人外出同样恢复体力，同时提供属性、干劲、Hint、隐藏风味和事件链进度；
    /// 仅替换本来就会消耗的休息回合，不为了赶链强行覆盖高价值训练。
    pub friend_outing_replaces_rest: bool,

    /// 友人第三次外出时，当前体力低于该值就选择恢复 50 体力的选项。
    ///
    /// 否则保留事件通用评分，可选无回复的属性/PT选项。`0` 表示关闭该低体力保护。
    pub friend_outing3_recovery_vital: i32,
}'''
if s.count(anchor)!=1: raise SystemExit(f'config anchor {s.count(anchor)}')
s=s.replace(anchor,replace)
s=s.replace('''            y3_recovery_horizon: false,
''','''            y3_recovery_horizon: false,
            friend_outing_replaces_rest: false,
            friend_outing3_recovery_vital: 0,
''',1)

anchor='''            } else if token == "y3horizon" {
                local.y3_recovery_horizon = true
            } else if token == "failmodel" {
'''
replace='''            } else if token == "y3horizon" {
                local.y3_recovery_horizon = true
            } else if token == "friendrest" {
                local.friend_outing_replaces_rest = true
            } else if let Some(v) = token.strip_prefix("friend3v") {
                local.friend_outing3_recovery_vital = v.parse()?
            } else if token == "failmodel" {
'''
if s.count(anchor)!=1: raise SystemExit(f'parser anchor {s.count(anchor)}')
s=s.replace(anchor,replace)

# Production removes the empirically negative post budget and enables the low-risk rest substitution.
s=s.replace('''            local.y3_post_train_vital_target = 10;
            local.y3_vital_shortfall_weight = 8.0;
''','''            local.y3_post_train_vital_target = 0;
            local.y3_vital_shortfall_weight = 0.0;
''',1)
s=s.replace('''            local.y3_recovery_horizon = true;
''','''            local.y3_recovery_horizon = true;
            local.friend_outing_replaces_rest = true;
            local.friend_outing3_recovery_vital = 45;
''',1)
s=s.replace('''/// - 第三年只在体力崩盘会损失后续普通训练回合时收费；有马前可控到 0，随后由赛后 +40 与超级拉面每回合 +20 接管。
''','''/// - 第三年终盘允许有马前把体力控到 0，随后由赛后 +40 与超级拉面每回合 +20 接管；
/// - 本来要休息且友人外出可用时，以友人外出替代纯休息；第三次外出低于 45 体力时选择回 50 体。
''',1)

# Helper converts either hard-gate rest or scored rest into friend outing, but never overrides training.
anchor='''    fn decide_train(&self, g: &RamenGame, a: &[RamenAction]) -> Result<(usize, Vec<RamenPolicyOutput>)> {
        let (mut guard, mut out) = self.policy.decide_train(g, a)?;
'''
replace='''    fn decide_train(&self, g: &RamenGame, a: &[RamenAction]) -> Result<(usize, Vec<RamenPolicyOutput>)> {
        let (mut guard, mut out) = self.policy.decide_train(g, a)?;
        if self.config.friend_outing_replaces_rest
            && a.get(guard).is_some_and(|x| x.operation == Operation::Rest)
            && let Some(friend_idx) = a.iter().position(|x| x.operation == Operation::FriendOuting)
        {
            // 不新增恢复回合，只把已经决定的纯休息换成收益更完整的友人外出。
            guard = friend_idx;
            if out.len() == a.len() {
                out[friend_idx].reason = "友人出行：替代原定休息并推进事件链".to_string();
                out[friend_idx].score = out.iter().map(|x| x.score).fold(f32::NEG_INFINITY, f32::max) + 1.0;
            } else {
                out = vec![RamenPolicyOutput {
                    score: f32::MAX,
                    reason: "守门: 友人出行替代低体力休息".to_string(),
                    ..Default::default()
                }];
            }
        }
'''
if s.count(anchor)!=1: raise SystemExit(f'decide anchor {s.count(anchor)}')
s=s.replace(anchor,replace)

# Event-specific recovery choice. Event ID is stable scenario data; only applies to outing3.
old='''    fn select_event_choice(
        &self, g: &RamenGame, _e: &EventData, c: &[Vec<EventChoice>], r: &mut StdRng,
    ) -> Result<usize> {
        self.select_choice(g, c, r)
    }
'''
new='''    fn select_event_choice(
        &self, g: &RamenGame, e: &EventData, c: &[Vec<EventChoice>], r: &mut StdRng,
    ) -> Result<usize> {
        if e.id == 830305113
            && self.config.friend_outing3_recovery_vital > 0
            && g.uma.vital < self.config.friend_outing3_recovery_vital
            && !c.is_empty()
        {
            // 友人外出3选项1固定恢复50体；在低体力恢复场景中不能被高PT权重误选成无回复选项。
            return Ok(0);
        }
        self.select_choice(g, c, r)
    }
'''
if s.count(old)!=1: raise SystemExit(f'event method {s.count(old)}')
s=s.replace(old,new)

p.write_text(s)
