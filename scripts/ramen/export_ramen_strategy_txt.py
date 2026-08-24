#!/usr/bin/env python3
"""Export the current ramen strategy and its dependencies as one reviewable TXT.

This script only reads repository files and writes exports/*.txt.  It never edits Rust source,
Cargo metadata, configuration, or production workflows, so the snapshot cannot affect builds.
"""
from datetime import datetime, timezone
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parents[1]
OUT = ROOT / "exports" / "ramen-strategy-snapshot-v29.txt"

# Ordered from policy entry points through rules/search/bench context.  The pending v29 patch is
# included explicitly because the v29 workflow failed before applying it; reviewers can distinguish
# committed runtime code from proposed-but-unapplied changes.
FILES = [
    "crates/umasim/src/game/ramen/policy.rs",
    "crates/umasim/src/trainer/local_ramen_trainer.rs",
    "crates/umasim/src/trainer/ramen_handwritten_trainer.rs",
    "crates/umasim/src/trainer/mod.rs",
    "crates/umasim/src/game/ramen/mod.rs",
    "crates/umasim/src/game/ramen/action.rs",
    "crates/umasim/src/game/ramen/effects.rs",
    "crates/umasim/src/game/ramen/events.rs",
    "crates/umasim/src/game/ramen/rules.rs",
    "crates/umasim/src/game/ramen/state.rs",
    "crates/umasim/src/game/ramen/game.rs",
    "crates/umasim/src/game/traits.rs",
    "crates/umasim/src/game/uma.rs",
    "crates/umasim/src/gamedata/ramen.rs",
    "crates/umasim/src/gamedata/event.rs",
    "crates/umasim/src/search/searchable.rs",
    "crates/umasim/src/search/flat_search.rs",
    "crates/umasim/src/bench.rs",
    "crates/umasim/src/bin/skill_pt_phase_matrix.rs",
    "crates/umasim/src/output/diagnostic.rs",
    "gamedata/scenario_ramen.json",
    ".trae/documents/ramen_memo_cn.md",
    "docs/legacy-ai-gap-audit.md",
    "scripts/restore_dynamic_friend_outing_v28.py",
    "scripts/restore_simplified_strategy_v29.py",
]


def git(*args: str) -> str:
    try:
        return subprocess.check_output(["git", *args], cwd=ROOT, text=True).strip()
    except Exception:
        return "unknown"


header = f"""UMA-AI 拉面杯策略代码与注释独立快照
========================================

生成时间（UTC）：{datetime.now(timezone.utc).isoformat()}
分支：{git('branch', '--show-current')}
提交：{git('rev-parse', 'HEAD')}

用途
----
这是供离线审阅/下载的纯文本归档，不参与 Rust mod、Cargo 构建或程序运行。
文件位于 exports/，主代码不会 import/include 它，因此不会与生产代码冲突。

状态说明
--------
1. “CURRENT SOURCE”章节是生成时分支上实际提交的代码。
2. v28 动态友人逻辑已经在当前 Rust 源码中。
3. v29 workflow run 32612728619 在执行补丁脚本时失败，故
   scripts/restore_simplified_strategy_v29.py 作为“待应用方案”收录，不能误认为已生效。
4. 文本保留每个源文件原注释和完整内容，章节边界标出仓库相对路径。
5. 这不是 PR 清理后的最终文件清单；其中包含实验策略、搜索适配、规则、数据和审计资料。

收录范围
--------
- 基础手写策略 RamenPolicy
- 当前增强手写策略 LocalRamenTrainer / RecommendedRamenTrainer
- 旧拉面手写壳 RamenHandwrittenTrainer
- Trainer 接口与导出
- 拉面动作、阶段、规则、效果、事件、状态与完整 Game 实现
- Uma/事件数据结构、拉面数据定义
- FlatSearch 拉面适配与搜索实现
- benchmark 和矩阵入口
- diag! 编译期日志机制
- scenario_ramen.json 规则数据
- 拉面机制中文备忘、历史逻辑缺口审计
- v28 已应用补丁脚本、v29 未成功应用的待选补丁脚本

"""

OUT.parent.mkdir(parents=True, exist_ok=True)
with OUT.open("w", encoding="utf-8", newline="\n") as out:
    out.write(header)
    for n, rel in enumerate(FILES, 1):
        path = ROOT / rel
        out.write("\n" + "=" * 100 + "\n")
        out.write(f"SECTION {n:02d}/{len(FILES):02d} — {rel}\n")
        out.write("=" * 100 + "\n\n")
        if not path.is_file():
            out.write(f"[MISSING AT EXPORT TIME] {rel}\n")
            continue
        out.write(path.read_text(encoding="utf-8"))
        out.write("\n")

print(f"wrote {OUT.relative_to(ROOT)} ({OUT.stat().st_size} bytes, {len(FILES)} sections)")
