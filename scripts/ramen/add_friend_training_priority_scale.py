from pathlib import Path
import os

scale = float(os.environ.get("FRIEND_TRAIN_SCALE", "1"))
p = Path("crates/umasim/src/trainer/local_ramen_trainer.rs")
s = p.read_text()
anchor = """            local.status_reserve_max = 40.0;
"""
insert = f"""            // 实验：统一缩放友人出现在训练位时的首次点击、解锁前和活跃期价值。
            // 不改变友人外出事件、万能材料价值、外出配额或其他训练评分。
            local.first_friend_click_value *= {scale:.8};
            local.low_friend_bond_value *= {scale:.8};
            local.active_friend_value *= {scale:.8};
            local.status_reserve_max = 40.0;
"""
if s.count(anchor) != 1:
    raise SystemExit(f"目标标记匹配数量错误: {s.count(anchor)}")
s = s.replace(anchor, insert, 1)
p.write_text(s)
print(f"友人训练优先级倍率={scale}")
