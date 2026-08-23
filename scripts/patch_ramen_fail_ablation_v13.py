from pathlib import Path
import re

def replace_once(text, pattern, replacement, label):
    text, count = re.subn(pattern, replacement, text, count=1, flags=re.MULTILINE)
    if count != 1:
        raise SystemExit(f"{label} count={count}")
    return text

# Make the canonical policy failure model switchable for a clean v8-window ablation.
p = Path("crates/umasim/src/game/ramen/policy.rs")
s = p.read_text()
s = replace_once(
    s,
    r"(    pub failure_penalty: f32,\n)",
    r"\1    /// Whether policy scoring applies ramen_basic_effect.fail_rate_drop.\n    pub effective_ramen_failure: bool,\n",
    "policy config field",
)
s = replace_once(
    s,
    r"(            failure_penalty: 60\.0,\n)",
    r"\1            effective_ramen_failure: true,\n",
    "policy default",
)
s = replace_once(
    s,
    r"                let fail_rate =\s*\n?\s*\(base_fail_rate \* \(100\.0 - ramen_effect\.fail_rate_drop as f32\) / 100\.0\)\.clamp\(0\.0, 100\.0\);",
    """                let fail_rate = if self.config.effective_ramen_failure {
                    (base_fail_rate * (100.0 - ramen_effect.fail_rate_drop as f32) / 100.0)
                        .clamp(0.0, 100.0)
                } else {
                    base_fail_rate
                };""",
    "policy effective block",
)
p.write_text(s)

# Toggle the local extra expected-failure layer at the same time.
p = Path("crates/umasim/src/trainer/local_ramen_trainer.rs")
s = p.read_text()
s = replace_once(
    s,
    r"(    pub ramen_window_weight: f32,\n)",
    r"\1    /// Match the canonical policy switch for the local expected-failure layer.\n    pub effective_ramen_failure: bool,\n",
    "local config field",
)
s = replace_once(
    s,
    r"(            ramen_window_weight: 0\.0,\n)",
    r"\1            effective_ramen_failure: true,\n",
    "local default",
)
s = replace_once(
    s,
    r'(            if token == "failmodel" \{\n)',
    '''            if token == "rawfail" {
                policy.effective_ramen_failure = false;
                local.effective_ramen_failure = false
            } else if token == "failmodel" {
''',
    "rawfail token",
)
s = replace_once(
    s,
    r"            let fr =\s*\n?\s*\(base_fr \* \(100\.0 - ramen_effect\.fail_rate_drop as f32\) / 100\.0\)\.clamp\(0\.0, 100\.0\);",
    """            let fr = if self.config.effective_ramen_failure {
                (base_fr * (100.0 - ramen_effect.fail_rate_drop as f32) / 100.0)
                    .clamp(0.0, 100.0)
            } else {
                base_fr
            };""",
    "local effective block",
)
p.write_text(s)
