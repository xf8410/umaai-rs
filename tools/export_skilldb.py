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
   card_id → [{skill_id, kind}]。2026-09-17 修正：skill_id = hint_value_1
   （限 hint_gain_type=0，363 个技能 100% 命中 skillDB）；hint_id（9081xxx 段）
   是 hint 版式 id 不是技能。kind 统一 0（白圈 9 折近似，见函数内注释）。
   用途：--pool deck-hint 口径的可买集合与折扣（终局买技能的候选池）。

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
    # 2026-09-17 修正：skill_id 取 hint_value_1（限 hint_gain_type=0 的行）。
    # 实测 master.mdb：hint_gain_type=0 共 3805 行、363 个唯一技能 id，100% 命中
    # skillDB（带 grade/cost）；hint_gain_type=1 的 1139 行 hint_value_1 ∈ {1..5,30}
    # 是非技能增益，过滤。hint_id（9081xxx 段）只是 hint 版式 id，不是技能——
    # 旧版错把 hint_id 存成 skill_id，导致 deck-hint 候选池全部无法定价/评分，
    # 终局买技能空转（实测：buyable 全 0）。
    # 折扣近似：hint 行不区分白/金圈等级，统一按白圈 9 折（kind=0 → 10% off），
    # 与 score_explain::deck_hint_discounts 的 kind 映射一致。
    hints = defaultdict(list)
    for cid, skill_id in cur.execute(
        "select support_card_id, hint_value_1 from single_mode_hint_gain "
        "where hint_gain_type = 0"
    ):
        hints[str(cid)].append({"skill_id": skill_id, "kind": 0})
    # (卡, 技能) 去重（同一 hint 类型多 hint_group 行只代表同一技能重复获得）
    for cid in hints:
        seen = set()
        uniq = []
        for e in hints[cid]:
            if e["skill_id"] not in seen:
                seen.add(e["skill_id"])
                uniq.append(e)
        hints[cid] = sorted(uniq, key=lambda x: x["skill_id"])
    (GD / "hintDB.json").write_text(
        json.dumps(hints, ensure_ascii=False, separators=(",", ":")), encoding="utf-8"
    )
    n_cards = sum(1 for v in hints.values() if v)
    n_skills = len({e["skill_id"] for v in hints.values() for e in v})
    print(f"hintDB.json: {n_cards} 张支援卡自带 hint 技能，唯一技能 {n_skills} 个")

    # ---- 3) ura_status_to_point.json：从 URA 源码抓（需联网或本地 clone）----
    ura_file = GD / "ura_status_to_point.json"
    if ura_file.exists():
        print(f"ura_status_to_point.json: 已存在（{len(json.loads(ura_file.read_text()))} 档），跳过")
    else:
        print("提示: ura_status_to_point.json 不存在，请按模块头注释从 URA 开源仓库导出"
              "（tools/export_ura_table.py 或手工）")


if __name__ == "__main__":
    main()
