#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""导出评分解释层数据文件（可复现脚本）。

三个产物的来源与用途（都在 gamedata/ 下）：

1. skillDB.json —— 技能主表，来源游戏 master.mdb：
   - skill_data：id/组/稀有度/评分增量(grade_value)/图标/排序/单模式禁用位
   - single_mode_skill_need_point：购买价格（need_skill_point）
   - text_data category=47：日文名；中文名沿用仓库已有 text_data_dict.json['47']
   用途：score_explain 的技能池（价格、评分增量、名称）。

2. hintDB.json —— 支援卡自带 hint 技能，来源 single_mode_hint_gain：
   card_id → [{skill_id, type}]，type 0=白圈 hint（近似打 9 折）、1=金圈（8 折）。
   用途：--pool deck-hint 口径的可买集合与折扣。

3. ura_status_to_point.json —— URA（UmamusumeResponseAnalyzer）开源仓库
   Database.cs 内嵌的 StatusToPoint 表（0..2500 档），属性值 → 评价点。
   用途：URA 口径的属性评分。

用法：python3 tools/export_skilldb.py /path/to/master.mdb
（cwd 需在仓库根；缺省从 ../.. 找 master.mdb）
"""

import json
import sqlite3
import sys
from collections import defaultdict
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
GD = ROOT / "gamedata"


def main() -> None:
    mdb = Path(sys.argv[1]) if len(sys.argv) > 1 else ROOT.parent / "master.mdb"
    if not mdb.exists():
        # 兜底：常见位置
        for cand in [ROOT.parent.parent / "master.mdb", Path.cwd() / "master.mdb"]:
            if cand.exists():
                mdb = cand
                break
    if not mdb.exists():
        sys.exit(f"找不到 master.mdb（传参或放到仓库上级目录）: {mdb}")

    conn = sqlite3.connect(str(mdb))
    cur = conn.cursor()

    # ---- 1) skillDB.json ----
    rows = cur.execute(
        "select id, group_id, rarity, group_rate, grade_value, icon_id, disp_order,"
        " disable_singlemode from skill_data"
    ).fetchall()
    prices = dict(
        cur.execute("select id, need_skill_point from single_mode_skill_need_point").fetchall()
    )
    zh = json.loads((GD / "text_data_dict.json").read_text(encoding="utf-8"))["47"]
    jp = {
        str(r[0]): r[1]
        for r in cur.execute('select "index", text from text_data where category=47')
    }
    skills = [
        {
            "id": sid,
            "name_zh": zh.get(str(sid), ""),
            "name_jp": jp.get(str(sid), ""),
            "rarity": rar,
            "group_id": gid,
            "group_rate": grate,
            "grade": grade,
            "cost": prices.get(sid, 0),
            "icon_id": icon,
            "disp_order": disp,
            "disable_singlemode": dis,
        }
        for (sid, gid, rar, grate, grade, icon, disp, dis) in rows
    ]
    (GD / "skillDB.json").write_text(
        json.dumps(skills, ensure_ascii=False, separators=(",", ":")), encoding="utf-8"
    )
    buyable = sum(
        1
        for s in skills
        if s["disable_singlemode"] == 0 and s["cost"] > 0 and s["grade"] > 0 and s["rarity"] in (1, 2)
    )
    print(f"skillDB.json: {len(skills)} 技能（可买池 {buyable}）")

    # ---- 2) hintDB.json ----
    hints = defaultdict(list)
    for cid, hid, kind in cur.execute(
        "select support_card_id, hint_id, hint_gain_type from single_mode_hint_gain"
    ):
        hints[str(cid)].append({"skill_id": hid, "kind": kind})
    (GD / "hintDB.json").write_text(
        json.dumps(hints, ensure_ascii=False, separators=(",", ":")), encoding="utf-8"
    )
    print(f"hintDB.json: {len(hints)} 张支援卡自带 hint 技能")

    # ---- 3) ura_status_to_point.json：从 URA 源码抓（需联网或本地 clone）----
    ura_file = GD / "ura_status_to_point.json"
    if ura_file.exists():
        print(f"ura_status_to_point.json: 已存在（{len(json.loads(ura_file.read_text()))} 档），跳过")
    else:
        print("提示: ura_status_to_point.json 不存在，请按模块头注释从 URA 开源仓库导出"
              "（tools/export_ura_table.py 或手工）")


if __name__ == "__main__":
    main()
