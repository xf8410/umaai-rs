from pathlib import Path

# Policy's canonical train score must use the failure rate after ramen_basic_effect.
p=Path('crates/umasim/src/game/ramen/policy.rs')
s=p.read_text()
s=s.replace(
'''use super::rules::{calc_ramen_pt_gain, get_region_range, get_super_ramen_clone_train_options};''',
'''use super::{
    effects::calc_ramen_training_effect,
    rules::{calc_ramen_pt_gain, get_region_range, get_super_ramen_clone_train_options},
};''')
old='''                let fail_rate = game.calc_training_failure_rate(&buffs, train);'''
new='''                let base_fail_rate = game.calc_training_failure_rate(&buffs, train);
                let ramen_effect = calc_ramen_training_effect(game, train, game.shining_count(train) > 0);
                // fail_rate_drop is a relative percentage reduction shared by every training
                // while eating: Y1 30%, Y2 50%, Y3 100%.
                let fail_rate = (base_fail_rate * (100.0 - ramen_effect.fail_rate_drop as f32) / 100.0)
                    .clamp(0.0, 100.0);'''
if s.count(old) != 1:
    raise SystemExit(f'policy failure-rate site count={s.count(old)}')
s=s.replace(old,new)
p.write_text(s)

# Local long-term/failure layers must use the same effective rate, otherwise they re-add
# a penalty based on the pre-ramen rate and cancel the lookahead's safety value.
p=Path('crates/umasim/src/trainer/local_ramen_trainer.rs')
s=p.read_text()
s=s.replace(
'''            policy::{RamenPolicy, RamenPolicyConfig, RamenPolicyOutput},
            rules::{calc_ramen_pt_gain, calc_region_bonus, list_special_targets_for},''',
'''            effects::calc_ramen_training_effect,
            policy::{RamenPolicy, RamenPolicyConfig, RamenPolicyOutput},
            rules::{calc_ramen_pt_gain, calc_region_bonus, list_special_targets_for},''')
old='''            let fr = g.calc_training_failure_rate(&buffs, tr);'''
new='''            let base_fr = g.calc_training_failure_rate(&buffs, tr);
            let ramen_effect = calc_ramen_training_effect(g, tr, g.shining_count(tr) > 0);
            let fr = (base_fr * (100.0 - ramen_effect.fail_rate_drop as f32) / 100.0)
                .clamp(0.0, 100.0);'''
if s.count(old) != 1:
    raise SystemExit(f'local failure-rate site count={s.count(old)}')
s=s.replace(old,new)
p.write_text(s)
