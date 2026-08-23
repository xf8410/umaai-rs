from pathlib import Path
p=Path('crates/umasim/src/trainer/local_ramen_trainer.rs')
s=p.read_text()
s=s.replace('''            local.friend_outing_replaces_rest = true;
            local.friend_outing3_recovery_vital = 45;
''','''            // 友人外出替代休息仍是实验项；正式 preset 不提前启用，避免与既有
            // FriendOuting 打分及事件选择逻辑重复叠加。
            local.friend_outing_replaces_rest = false;
            local.friend_outing3_recovery_vital = 0;
''',1)
s=s.replace('''/// - 本来要休息且友人外出可用时，以友人外出替代纯休息；第三次外出低于 45 体力时选择回 50 体。
''','''/// - 友人外出沿用既有候选打分与事件逻辑；“替代休息”仅保留为矩阵实验，尚未进入正式 preset。
''',1)
p.write_text(s)
