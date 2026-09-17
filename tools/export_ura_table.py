#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""从 URA 开源仓库源码提取 StatusToPoint 表（属性值→评价点，0..2500 档）。

数据来源（一手原文）：
https://github.com/UmamusumeResponseAnalyzer/UmamusumeResponseAnalyzer
仓库内 UmamusumeResponseAnalyzer/Database.cs 的 `StatusToPoint` 数组。

用法：python3 tools/export_ura_table.py [Database.cs 路径]
- 传了路径就直接解析该文件；
- 不传则尝试从 GitHub raw 拉取 master 分支（需要能联网）。
"""

import json
import re
import sys
import urllib.request
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
GD = ROOT / "gamedata"
RAW_URL = (
    "https://raw.githubusercontent.com/UmamusumeResponseAnalyzer/"
    "UmamusumeResponseAnalyzer/master/UmamusumeResponseAnalyzer/Database.cs"
)


def main() -> None:
    if len(sys.argv) > 1:
        src = Path(sys.argv[1]).read_text(encoding="utf-8")
    else:
        src = urllib.request.urlopen(RAW_URL, timeout=60).read().decode("utf-8")

    m = re.search(r"StatusToPoint.*?=\s*\[(.*?)\];", src, re.S)
    if not m:
        sys.exit("在 Database.cs 里没找到 StatusToPoint 数组（上游可能改了结构）")
    nums = [int(x) for x in re.findall(r"\d+", m.group(1))]
    out = GD / "ura_status_to_point.json"
    out.write_text(json.dumps(nums, separators=(",", ":")), encoding="utf-8")
    print(f"ura_status_to_point.json: {len(nums)} 档")
    print(f"锚点校验: pt[1]={nums[1]} pt[1200]={nums[1200]} pt[2500]={nums[2500]}")


if __name__ == "__main__":
    main()
