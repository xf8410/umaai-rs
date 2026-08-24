from pathlib import Path

p = Path("scripts/restore_simplified_strategy_v29.py")
s = p.read_text()

old_start = "# Remove race shortcut in run_ramen_select.\n"
old_end = "# Dynamic super ramen deterministic selection based on uncovered status gaps and deck/card affinity.\n"
i = s.find(old_start)
j = s.find(old_end, i)
if i < 0 or j < 0:
    raise SystemExit("v29 race patch section not found")

# This is a one-time migration helper. It writes valid Python source into the existing migration
# script. The generated source removes the COMPLETE Rust `if self.is_race_turn()` block by balanced
# braces, rather than stopping at the first `let actions` inside that block.
replacement = r'''# Remove the complete race shortcut in run_ramen_select. Match executable code inside the
# function and use balanced braces so nested statements cannot leave a malformed Rust fragment.
fn_marker = "    fn run_ramen_select<T: Trainer<Self>>("
fn_i = s.find(fn_marker)
if fn_i < 0:
    raise SystemExit("run_ramen_select function not found")
next_fn = s.find("\n    fn ", fn_i + len(fn_marker))
if next_fn < 0:
    next_fn = len(s)
body = s[fn_i:next_fn]
if_marker = "        if self.is_race_turn() {"
if_i = body.find(if_marker)
if if_i >= 0:
    depth = 0
    block_end = None
    for pos in range(if_i, len(body)):
        ch = body[pos]
        if ch == "{":
            depth += 1
        elif ch == "}":
            depth -= 1
            if depth == 0:
                block_end = pos + 1
                break
    if block_end is None:
        raise SystemExit("run_ramen_select race shortcut has unbalanced braces")
    while block_end < len(body) and body[block_end] in " \t\r\n":
        block_end += 1
    body = (
        body[:if_i]
        + "        // 固定比赛回合仍先经过选面/隐藏风味阶段；Train 阶段只提供比赛动作。\n"
        + body[block_end:]
    )
    s = s[:fn_i] + body + s[next_fn:]
else:
    print("run_ramen_select race shortcut already removed")
'''

updated = s[:i] + replacement + s[j:]
compile(updated, str(p), "exec")
p.write_text(updated)
print("v29 migration patch repaired and syntax-checked")
