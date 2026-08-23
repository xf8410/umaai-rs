from pathlib import Path

# Make the canonical policy failure model switchable for a clean v8-window ablation.
p=Path('crates/umasim/src/game/ramen/policy.rs')
s=p.read_text()
s=s.replace('''    pub failure_penalty: f32,
''','''    pub failure_penalty: f32,
    /// Whether policy scoring applies ramen_basic_effect.fail_rate_drop.
    pub effective_ramen_failure: bool,
''')
s=s.replace('''            failure_penalty: 60.0,
''','''            failure_penalty: 60.0,
            effective_ramen_failure: true,
''')
old='''                let fail_rate = (base_fail_rate * (100.0 - ramen_effect.fail_rate_drop as f32) / 100.0)
                    .clamp(0.0, 100.0);'''
new='''                let fail_rate = if self.config.effective_ramen_failure {
                    (base_fail_rate * (100.0 - ramen_effect.fail_rate_drop as f32) / 100.0)
                        .clamp(0.0, 100.0)
                } else {
                    base_fail_rate
                };'''
if s.count(old) != 1: raise SystemExit(f'policy effective block count={s.count(old)}')
s=s.replace(old,new)
p.write_text(s)

# Toggle the local extra expected-failure layer at the same time.
p=Path('crates/umasim/src/trainer/local_ramen_trainer.rs')
s=p.read_text()
s=s.replace('''    pub ramen_window_weight: f32,
''','''    pub ramen_window_weight: f32,
    /// Match the canonical policy switch for the local expected-failure layer.
    pub effective_ramen_failure: bool,
''')
s=s.replace('''            ramen_window_weight: 0.0,
''','''            ramen_window_weight: 0.0,
            effective_ramen_failure: true,
''')
s=s.replace('''            if token == "failmodel" {
''','''            if token == "rawfail" {
                policy.effective_ramen_failure = false;
                local.effective_ramen_failure = false
            } else if token == "failmodel" {
''')
old='''            let fr = (base_fr * (100.0 - ramen_effect.fail_rate_drop as f32) / 100.0)
                .clamp(0.0, 100.0);'''
new='''            let fr = if self.config.effective_ramen_failure {
                (base_fr * (100.0 - ramen_effect.fail_rate_drop as f32) / 100.0)
                    .clamp(0.0, 100.0)
            } else {
                base_fr
            };'''
if s.count(old) != 1: raise SystemExit(f'local effective block count={s.count(old)}')
s=s.replace(old,new)
p.write_text(s)
