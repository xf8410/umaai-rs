from pathlib import Path

path = Path('crates/umasim/src/trainer/local_ramen_trainer.rs')
text = path.read_text(encoding='utf-8')
old_doc = '''/// - 五段事件按当前体力、干劲、属性/PT及完链进度动态估值，第三段不再使用硬体力阈值。'''
new_doc = '''/// - 五段事件按当前体力、干劲、属性/PT及完链进度动态估值，第三段不再使用硬体力阈值；
/// - 不使用 RMJ 截止期紧迫度加分：300 局同种子矩阵中 deadline20/35/50 完全同轨，
///   平均分 56960.7，显著低于 deadline0 的 58881.6；硬目标仍由规则和既有跨线价值保证。'''
old_value = '            local.deadline_urgency_scale = 0.35;'
new_value = '            local.deadline_urgency_scale = 0.0;'
if text.count(old_doc) != 1:
    raise SystemExit('recommended preset documentation anchor missing or duplicated')
if text.count(old_value) != 1:
    raise SystemExit('recommended deadline value anchor missing or duplicated')
text = text.replace(old_doc, new_doc).replace(old_value, new_value)
path.write_text(text, encoding='utf-8')
