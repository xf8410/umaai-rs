from pathlib import Path

# ---------- policy: event values omitted by the simplified scorer ----------
p=Path('crates/umasim/src/game/ramen/policy.rs')
s=p.read_text()
old='''        val += c.value.status_pt[5] as f32 * self.config.pt_rate;
        val += c.value.vital as f32 * self.config.event_vital_weight;
        val += c.value.motivation as f32 * self.config.event_motivation_weight;
'''
new='''        val += c.value.status_pt[5] as f32 * self.config.pt_rate;
        val += c.value.vital as f32 * self.config.event_vital_weight;
        val += c.value.motivation as f32 * self.config.event_motivation_weight;
        // 旧简化器漏掉了 Hint、羁绊和永久最大体力，导致友人/支援事件被系统性低估。
        val += c.value.hint_level as f32 * global!(GAMECONSTANTS).hint_pt_rate * self.config.pt_rate;
        val += c.value.friendship as f32 * 5.0;
        val += c.value.max_vital as f32 * self.config.event_vital_weight * 2.0;
'''
if s.count(old)!=1: raise SystemExit(f'event score anchor={s.count(old)}')
s=s.replace(old,new,1)
p.write_text(s)

# ---------- local policy: dynamic friend guard, deadline, special targets, all-hint ----------
p=Path('crates/umasim/src/trainer/local_ramen_trainer.rs')
s=p.read_text()
# New knobs
anchor='''    pub friend_rest_max_special: i32,
}'''
repl='''    pub friend_rest_max_special: i32,

    /// RMJ/第三年5000目标在截止前的可达性紧迫度。
    pub deadline_urgency_scale: f32,

    /// SpecialSelect 是否按吃后库存、后续可制作集合和年末剩余价值动态选择。
    pub dynamic_special_targets: bool,
}'''
if s.count(anchor)!=1: raise SystemExit('config fields')
s=s.replace(anchor,repl,1)
s=s.replace('''            friend_rest_max_special: 4,
''','''            friend_rest_max_special: 4,
            deadline_urgency_scale: 0.0,
            dynamic_special_targets: false,
''',1)
# parser
anchor='''            } else if token == "failmodel" {
'''
repl='''            } else if let Some(v) = token.strip_prefix("deadline") {
                local.deadline_urgency_scale = v.parse::<f32>()? / 100.0
            } else if token == "specialdynamic" {
                local.dynamic_special_targets = true
            } else if token == "failmodel" {
'''
if s.count(anchor)!=1: raise SystemExit('parser')
s=s.replace(anchor,repl,1)

# Generic event now includes max vital: prevent dynamic friend double count.
s=s.replace('''                adjust += c.value.max_vital as f32 * self.policy.config.event_vital_weight * prob;
''','',1)

# Replace friend-rest block: expand the hard-rest result into a full table, but remember that only
# recovery actions may win. This makes dynamic friend valuation actually execute without allowing
# a risky training to bypass the low-vital guard.
old='''        let (mut guard, mut out) = self.policy.decide_train(g, a)?;
        if self.config.friend_outing_replaces_rest
            && self.friend_outing_within_pacing(g)
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
new='''        let (mut guard, mut out) = self.policy.decide_train(g, a)?;
        let recovery_guard = self.config.friend_outing_replaces_rest
            && a.get(guard).is_some_and(|x| x.operation == Operation::Rest)
            && out.len() != a.len();
        if recovery_guard && a.iter().any(|x| x.operation == Operation::FriendOuting) {
            // 展开完整候选以便真正执行五段动态估值；最终仍只允许休息/友人恢复动作获胜。
            out = self.policy.score_train_actions(g, a)?;
            guard = a.iter().position(|x| x.operation == Operation::Rest).unwrap_or(guard);
        }
'''
if s.count(old)!=1: raise SystemExit(f'friend guard={s.count(old)}')
s=s.replace(old,new,1)
# before final cap, enforce recovery choices
anchor='''        if !self.friend_outing_within_pacing(g) && a.get(c).is_some_and(|x| x.operation == Operation::FriendOuting) {
'''
insert='''        if recovery_guard {
            c = out
                .iter()
                .enumerate()
                .filter(|(i, _)| {
                    a.get(*i).is_some_and(|x| {
                        x.operation == Operation::Rest
                            || (x.operation == Operation::FriendOuting && self.friend_outing_within_pacing(g))
                    })
                })
                .max_by(|(li, l), (ri, r)| l.score.total_cmp(&r.score).then_with(|| ri.cmp(li)))
                .map(|(i, _)| i)
                .ok_or_else(|| anyhow::anyhow!("低体力守门没有合法恢复动作"))?;
        }
        if !self.friend_outing_within_pacing(g) && a.get(c).is_some_and(|x| x.operation == Operation::FriendOuting) {
'''
if s.count(anchor)!=1: raise SystemExit('recovery final')
s=s.replace(anchor,insert,1)

# Third-year hint_special is all-trigger, not random-one. Also account for per-card repeats.
old='''            let hp = if self.config.probabilistic_hint && hn > 0 {
                1. / hn as f32
            } else {
                1.
            };
'''
new='''            let all_hint = g.is_hint_special_active_for_train(tr);
            let hp = if self.config.probabilistic_hint && hn > 0 && !all_hint {
                1. / hn as f32
            } else {
                1.
            };
'''
if s.count(old)!=1: raise SystemExit('hint hp')
s=s.replace(old,new,1)
# augment hint bonus by repeat count in both branches (deck index is person index for support cards)
s=s.replace('''                        if x.hint() {
                            lt += self.config.hint_bonus * hp
                        }
''','''                        if x.hint() {
                            let repeats = if all_hint && i < g.deck().len() {
                                1 + g.deck()[i].effect.hint_count_bonus
                            } else { 1 };
                            lt += self.config.hint_bonus * hp * repeats as f32
                        }
''',1)
s=s.replace('''                    PersonType::Card if x.hint() => lt += self.config.hint_bonus * hp,
''','''                    PersonType::Card if x.hint() => {
                        let repeats = if all_hint && i < g.deck().len() {
                            1 + g.deck()[i].effect.hint_count_bonus
                        } else { 1 };
                        lt += self.config.hint_bonus * hp * repeats as f32
                    }
''',1)

# Deadline urgency helper and dynamic special selector.
anchor='''    fn decide_ramen(&self, g: &RamenGame, a: &[RamenAction]) -> Result<(usize, Vec<RamenPolicyOutput>)> {
'''
helpers='''    fn deadline_urgency(&self, g: &RamenGame, post: i32) -> Result<f32> {
        if self.config.deadline_urgency_scale <= 0.0 { return Ok(0.0); }
        let year = (g.current_year() - 1) as usize;
        let data = RAMENDATA.get().ok_or_else(|| anyhow::anyhow!("RAMENDATA 未初始化"))?;
        let normal = *data.ramen_success_pt.get(year).unwrap_or(&i32::MAX);
        let target = if year == 2 { 5000 } else { normal };
        if post >= target { return Ok(0.0); }
        let turns = (Self::year_end(g) - g.turn() + 1).max(1) as f32;
        let gain = calc_ramen_pt_gain(year, g.ramen.eat_count + 1)?.max(1) as f32;
        let bowls_needed = ((target - post) as f32 / gain).ceil();
        let pressure = (bowls_needed / turns).clamp(0.0, 1.5);
        Ok(pressure * (target - post) as f32 * self.config.deadline_urgency_scale)
    }

    fn decide_special_dynamic(&self, g: &RamenGame, a: &[RamenAction]) -> Result<(usize, Vec<RamenPolicyOutput>)> {
        let (_, mut out) = self.policy.decide_special(g, a)?;
        for (act, score) in a.iter().zip(out.iter_mut()) {
            let Some(targets) = act.special_targets else { continue };
            let Some(region) = act.ramen else { continue };
            let mut post = g.ramen.clone();
            consume_for_ramen(&mut post, region, &targets)?;
            let craftable = g.ramen.selected_regions.iter().filter(|&&rid| {
                list_special_targets_for(&post, rid).map(|x| !x.is_empty()).unwrap_or(false)
            }).count() as f32;
            let balance = post.feeling_stock.iter().map(|&x| (x as f32 + 2.0).sqrt()).sum::<f32>();
            let year_left = (Self::year_end(g) - g.turn()).max(0) as f32 / 21.0;
            let future = (craftable * 18.0 + balance * 4.0) * year_left;
            score.score += future;
            score.add("future_craftability", future);
            score.reason = format!("隐藏方案{:?} 后续可做{}种", targets, craftable as i32);
        }
        Ok((Self::choose(&out), out))
    }

'''
if s.count(anchor)!=1: raise SystemExit('helpers anchor')
s=s.replace(anchor,helpers+anchor,1)

# Relax eatguard on race/nontrain only when deadline pressure makes eating valuable.
old='''        if self.config.eat_requires_training && !matches!(self.pre_eat_action(g)?, Operation::Train(_)) {
'''
new='''        let pre_action = self.pre_eat_action(g)?;
        let year = (g.current_year() - 1) as usize;
        let eat_post = g.ramen.scenario_pt + calc_ramen_pt_gain(year, g.ramen.eat_count)?;
        let deadline_exception = self.deadline_urgency(g, eat_post)? > 0.0
            && matches!(pre_action, Operation::Race | Operation::Rest | Operation::FriendOuting);
        if self.config.eat_requires_training && !matches!(pre_action, Operation::Train(_)) && !deadline_exception {
'''
if s.count(old)!=1: raise SystemExit('eat guard')
s=s.replace(old,new,1)
# Add urgency into each ramen candidate
old='''                let (ck, rmj, great) = self.scenario_threshold_value(g, post)?;
'''
new='''                let (ck, rmj, great) = self.scenario_threshold_value(g, post)?;
                let deadline = self.deadline_urgency(g, post)?;
'''
if s.count(old)!=1: raise SystemExit('deadline compute')
s=s.replace(old,new,1)
s=s.replace('''                o.score += ck + rmj + great + window + safety + look - cook2_cost;
''','''                o.score += ck + rmj + great + deadline + window + safety + look - cook2_cost;
''',1)
s=s.replace('''                o.add("great_cross", great);
''','''                o.add("great_cross", great);
                o.add("deadline_urgency", deadline);
''',1)

# Route SpecialSelect dynamically.
s=s.replace('''            RamenStage::SpecialSelect => self.policy.decide_special(g, a)?,
''','''            RamenStage::SpecialSelect => {
                if self.config.dynamic_special_targets { self.decide_special_dynamic(g, a)? }
                else { self.policy.decide_special(g, a)? }
            }
''',1)
# production enable
s=s.replace('''            local.friend_rest_max_special = 4;
            LocalRamenTrainer::with_configs(policy, local)
''','''            local.friend_rest_max_special = 4;
            local.deadline_urgency_scale = 0.35;
            local.dynamic_special_targets = true;
            LocalRamenTrainer::with_configs(policy, local)
''',1)
p.write_text(s)

# ---------- game: permit ramen before fixed races, dynamic super ramen option ----------
p=Path('crates/umasim/src/game/ramen/game.rs')
s=p.read_text()
# Only Train is race-only; RamenSelect/SpecialSelect remain available.
s=s.replace('''        if self.is_race_turn() {
            return Ok(vec![RamenAction::no_ramen(Operation::Race)]);
        }
''','''        if self.is_race_turn() && self.stage == RamenStage::Train {
            return Ok(vec![RamenAction::no_ramen(Operation::Race)]);
        }
''',1)
# Remove the complete race shortcut in run_ramen_select. Match executable code inside the
# function and use balanced braces so nested statements cannot leave a malformed Rust fragment.
fn_marker = "    fn run_ramen_select<T: Trainer<Self>>("
fn_i = s.find(fn_marker)
if fn_i < 0:
    raise SystemExit("run_ramen_select function not found")
next_fn = s.find("\n    fn ", fn_i + len(fn_marker))
if next_fn < 0:
    next_fn = len(s)
body = s[fn_i:next_fn]
if_marker = "        if self.is_race_turn() {"
if_i = body.find(if_marker)
if if_i >= 0:
    depth = 0
    block_end = None
    for pos in range(if_i, len(body)):
        ch = body[pos]
        if ch == "{":
            depth += 1
        elif ch == "}":
            depth -= 1
            if depth == 0:
                block_end = pos + 1
                break
    if block_end is None:
        raise SystemExit("run_ramen_select race shortcut has unbalanced braces")
    while block_end < len(body) and body[block_end] in " \t\r\n":
        block_end += 1
    body = (
        body[:if_i]
        + "        // 固定比赛回合仍先经过选面/隐藏风味阶段；Train 阶段只提供比赛动作。\n"
        + body[block_end:]
    )
    s = s[:fn_i] + body + s[next_fn:]
else:
    print("run_ramen_select race shortcut already removed")
# Dynamic super ramen deterministic selection based on uncovered status gaps and deck/card affinity.
old='''    fn run_super_ramen_select(&mut self) -> Result<()> {
        let _option = fixed_super_ramen_selection()?;
        self.ramen.super_ramen = Some(1); // 选项二（索引 1）
        diag!("超级拉面选择: 选项二");
        Ok(())
    }
'''
new='''    fn run_super_ramen_select(&mut self) -> Result<()> {
        let options = rules::get_super_ramen_clone_train_options()?;
        let mut best = 0usize;
        let mut best_value = f32::NEG_INFINITY;
        for (idx, trains) in options.iter().enumerate() {
            let mut value = 0.0;
            for &t in trains {
                if !(0..5).contains(&t) { continue; }
                let t = t as usize;
                let gap = (self.uma.five_status_limit[t] - self.uma.five_status[t]).max(0) as f32;
                let cards = self.deck.iter().filter(|c| c.card_type == t as i32).count() as f32;
                value += gap.min(600.0) + cards * 120.0;
            }
            if value > best_value { best_value = value; best = idx; }
        }
        self.ramen.super_ramen = Some(best);
        diag!("超级拉面动态选择: 选项{} value={:.0}", best + 1, best_value);
        Ok(())
    }
'''
if s.count(old)!=1: raise SystemExit(f'super selector={s.count(old)}')
s=s.replace(old,new,1)
# remove unused import fixed selector
s=s.replace('''    policy::fixed_super_ramen_selection,
''','')
p.write_text(s)
