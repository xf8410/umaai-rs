from pathlib import Path
import os

unit = float(os.environ.get("FRIEND_MATERIAL_UNIT", "0"))
p = Path("crates/umasim/src/trainer/local_ramen_trainer.rs")
s = p.read_text()
old = """        let material = self.policy.config.friend_outing_bonus * (2.0 / 3.0);
"""
new = f"""        // 实验：只评价实际进入库存的万能材料。库存上限为4；库存3只算1个，库存4算0。
        let material_units = (4 - g.ramen.special_feeling).clamp(0, 2) as f32;
        let material = material_units * {unit:.8};
"""
if s.count(old) != 1:
    raise SystemExit(f"材料估值标记匹配数量错误: {s.count(old)}")
s = s.replace(old, new, 1)
s = s.replace(
    '"友人外出#{} 选项{} 动态事件{:.0} 材料+2(库存{}也不禁用)",',
    '"友人外出#{} 选项{} 动态事件{:.0} 实际入库材料{:.0}个(库存{})",',
    1,
).replace(
    "                event_value,\n                g.ramen.special_feeling\n",
    "                event_value,\n                material_units,\n                g.ramen.special_feeling\n",
    1,
)
p.write_text(s)
print(f"实际入库万能材料单价={unit}")
