from pathlib import Path

src = Path("crates/umasim/src/trainer/local_ramen_trainer.rs")
text = src.read_text(encoding="utf-8")
replacements = {
    "/// - 本来要休息时按 1/3/5 跨年累计节奏使用友人外出；即使万能材料暂时溢出也不禁止；":
    "/// - 本来要休息时按 0/2/5 跨年累计节奏使用友人外出；第一年不消耗次数，第二年累计 2 次，第三年完成 5 次；",
    "            local.friend_outing_cumulative_caps = [1, 3, 5];":
    "            local.friend_outing_cumulative_caps = [0, 2, 5];",
}
for old, new in replacements.items():
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"expected exactly one match, got {count}: {old!r}")
    text = text.replace(old, new)

test = r'''

#[cfg(test)]
mod tests {
    use super::RecommendedRamenTrainer;

    /// 正式 preset 必须使用 v44 同种子回归胜出的友人跨年节奏。
    #[test]
    #[allow(clippy::panic)]
    fn recommended_ramen_uses_025_friend_pacing() {
        let trainer = RecommendedRamenTrainer::new();
        let actual = trainer
            .years
            .each_ref()
            .map(|year| year.config.friend_outing_cumulative_caps);
        let expected = [[0, 2, 5]; 3];
        println!("正式友人累计出门配额: {actual:?}");
        if actual != expected {
            panic!("正式 preset 应使用 {expected:?}，实际为 {actual:?}");
        }
    }
}
'''
if "fn recommended_ramen_uses_025_friend_pacing()" not in text:
    text += test
src.write_text(text, encoding="utf-8")
