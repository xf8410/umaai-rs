from pathlib import Path
p=Path('crates/umasim/src/trainer/local_ramen_trainer.rs')
s=p.read_text()

# Add dynamic mode without deleting the v22 knobs, so controls remain reproducible.
anchor='''    pub y3_post_train_hard_floor: i32,
}'''
replace='''    pub y3_post_train_hard_floor: i32,

    /// 是否按“距离下一次确定恢复前还有几个可训练回合”判断第三年体力崩盘。
    ///
    /// 当前规则中 turn=70 训练后，turn=71 为有马纪念，赛后固定恢复 40；随后
    /// turn=72 起超级拉面每回合开始恢复 20。因此 turn=70 可以把体力控到 0，
    /// 不应再为训练后低体力付费。更早回合若低体力会影响至少一个普通训练回合，
    /// 才计入崩盘成本。
    pub y3_recovery_horizon: bool,
}'''
if s.count(anchor)!=1: raise SystemExit(f'cfg anchor {s.count(anchor)}')
s=s.replace(anchor,replace)
s=s.replace('''            y3_post_train_hard_floor: 0,
''','''            y3_post_train_hard_floor: 0,
            y3_recovery_horizon: false,
''',1)

anchor='''            } else if let Some(v) = token.strip_prefix("y3hard") {
                local.y3_post_train_hard_floor = v.parse()?
            } else if token == "failmodel" {
'''
replace='''            } else if let Some(v) = token.strip_prefix("y3hard") {
                local.y3_post_train_hard_floor = v.parse()?
            } else if token == "y3horizon" {
                local.y3_recovery_horizon = true
            } else if token == "failmodel" {
'''
if s.count(anchor)!=1: raise SystemExit(f'parser {s.count(anchor)}')
s=s.replace(anchor,replace)

# Production: v22 established pre-budget has no effect and post-budget slightly hurts mean.
# Keep only a modest post-collapse cost on turns where it can actually cost another training turn.
s=s.replace('''            local.y3_pre_train_vital_target = 30;
            local.y3_post_train_vital_target = 10;
            local.y3_vital_shortfall_weight = 8.0;
            local.y3_post_train_hard_floor = 0;
''','''            local.y3_pre_train_vital_target = 0;
            local.y3_post_train_vital_target = 10;
            local.y3_vital_shortfall_weight = 8.0;
            local.y3_post_train_hard_floor = 0;
            local.y3_recovery_horizon = true;
''',1)
s=s.replace('''/// - 第三年逐碗预演训练前后体力，以软成本联合评价 `V0` 与 `V1`，避免单端硬门过度保守。
''','''/// - 第三年只在体力崩盘会损失后续普通训练回合时收费；有马前可控到 0，随后由赛后 +40 与超级拉面每回合 +20 接管。
''',1)

# Add helper near transition function.
anchor='''    fn post_ramen_vital_transition(&self, g: &RamenGame, region_id: usize) -> Result<Option<(usize, i32, i32)>> {
'''
helper='''    /// 第三年本回合训练后，低体力是否还会伤害下一次普通训练。
    ///
    /// turn=70 后紧接 turn=71 有马纪念（赛后 +40），再进入 turn=72 超级拉面（回合开始 +20），
    /// 所以没有待保护的普通训练回合；此时体力归零也是合理终盘控制。
    fn y3_collapse_matters(&self, g: &RamenGame) -> bool {
        !self.config.y3_recovery_horizon || g.turn() < 70
    }

'''
if s.count(anchor)!=1: raise SystemExit(f'helper anchor {s.count(anchor)}')
s=s.replace(anchor,helper+anchor)

# Apply soft budget only when collapse matters. Pre-budget remains independently supported for experiments.
old='''                    let pre_short = (self.config.y3_pre_train_vital_target - pre_vital).max(0) as f32;
                    let post_short = (self.config.y3_post_train_vital_target - post_vital).max(0) as f32;
                    let transition_cost = (pre_short + post_short) * self.config.y3_vital_shortfall_weight;
'''
new='''                    let pre_short = (self.config.y3_pre_train_vital_target - pre_vital).max(0) as f32;
                    let post_short = if self.y3_collapse_matters(g) {
                        (self.config.y3_post_train_vital_target - post_vital).max(0) as f32
                    } else {
                        0.0
                    };
                    let transition_cost = (pre_short + post_short) * self.config.y3_vital_shortfall_weight;
'''
if s.count(old)!=1: raise SystemExit(f'cost block {s.count(old)}')
s=s.replace(old,new)

p.write_text(s)
