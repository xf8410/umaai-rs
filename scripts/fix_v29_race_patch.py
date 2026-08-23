from pathlib import Path

p = Path("scripts/restore_simplified_strategy_v29.py")
s = p.read_text()

old = '''# Remove race shortcut in run_ramen_select.
start=\'\'\'        // race_turn 短路：直接执行比赛，跳过 SpecialSelect/Train 阶段
        if self.is_race_turn() {
\'\'\'
end=\'\'\'        let actions = self.list_actions()?;
\'\'\'
i=s.find(start)
if i<0: raise SystemExit('race shortcut start')
j=s.find(end,i)
if j<0: raise SystemExit('race shortcut end')
s=s[:i]+\'\'\'        // 固定比赛回合仍先经过选面/隐藏风味阶段；Train 阶段只提供比赛动作。\\n\'\'\'+s[j:]
'''

# This must be raw: it is Python source that will itself be written into another Python file.
# Without r'''...''', "\\n" becomes a literal newline inside a quoted string and causes the
# unterminated-string SyntaxError seen in run 32613115047.
new = r'''# Remove the race shortcut in run_ramen_select. Match executable code inside the function;
# comments are deliberately not part of the matcher.
fn_marker = "    fn run_ramen_select<T: Trainer<Self>>("
fn_i = s.find(fn_marker)
if fn_i < 0:
    raise SystemExit("run_ramen_select function not found")
next_fn = s.find("\n    fn ", fn_i + len(fn_marker))
if next_fn < 0:
    next_fn = len(s)
body = s[fn_i:next_fn]
if "if self.is_race_turn()" in body:
    if_i = body.find("        if self.is_race_turn() {")
    actions_i = body.find("        let actions = self.list_actions()?;", if_i)
    if if_i < 0 or actions_i < 0:
        raise SystemExit("run_ramen_select race shortcut boundaries not found")
    body = (
        body[:if_i]
        + "        // 固定比赛回合仍先经过选面/隐藏风味阶段；Train 阶段只提供比赛动作。\n"
        + body[actions_i:]
    )
    s = s[:fn_i] + body + s[next_fn:]
else:
    print("run_ramen_select race shortcut already removed")
'''

if s.count(old) != 1:
    raise SystemExit(f"old patch block count={s.count(old)}")

updated = s.replace(old, new, 1)
# Validate generated Python before allowing the workflow to execute it.
compile(updated, str(p), "exec")
p.write_text(updated)
print("v29 patch repaired and syntax-checked")
