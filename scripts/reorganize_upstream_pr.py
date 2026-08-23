from pathlib import Path
import shutil

ROOT = Path('.')


def move(src: str, dst: str) -> None:
    s, d = Path(src), Path(dst)
    if not s.exists():
        return
    d.parent.mkdir(parents=True, exist_ok=True)
    if d.exists():
        raise SystemExit(f'destination already exists: {d}')
    shutil.move(str(s), str(d))

# 1. Benchmark outputs are generated artifacts, not source.
shutil.rmtree('benchmark-results', ignore_errors=True)
gitignore = Path('.gitignore')
text = gitignore.read_text(encoding='utf-8') if gitignore.exists() else ''
if '/benchmark-results/' not in text.splitlines():
    text = text.rstrip() + '\n\n# Generated benchmark and training reports\n/benchmark-results/\n'
gitignore.write_text(text, encoding='utf-8')

# 2. Keep data-collection executables out of src/bin. Explicit Cargo targets retain names.
data_tools = [
    'ramen_feature_dataset',
    'ramen_low_score_diagnostic',
    'skill_pt_matrix',
    'skill_pt_phase_matrix',
    'y3_train_pt_trace',
]
for name in data_tools:
    move(f'crates/umasim/src/bin/{name}.rs', f'crates/umasim/tools/data_collection/{name}.rs')

cargo = Path('crates/umasim/Cargo.toml')
cargo_text = cargo.read_text(encoding='utf-8')
marker = '# Data-collection and label-generation tools are kept outside src/bin.'
if marker not in cargo_text:
    blocks = ['\n' + marker]
    for name in data_tools:
        blocks.append(f'''\n[[bin]]\nname = "{name}"\npath = "tools/data_collection/{name}.rs"\n''')
    insert_at = cargo_text.find('\n[features]')
    if insert_at < 0:
        raise SystemExit('Cargo.toml [features] anchor missing')
    cargo_text = cargo_text[:insert_at] + ''.join(blocks) + cargo_text[insert_at:]
cargo.write_text(cargo_text, encoding='utf-8')

# 3. Group project-specific Python utilities. Keep package directories untouched.
script_dir = Path('scripts')
ramen_scripts = script_dir / 'ramen'
ramen_scripts.mkdir(parents=True, exist_ok=True)
this_script = Path(__file__)
root_python = sorted(p for p in script_dir.glob('*.py') if p != this_script)
for p in root_python:
    move(str(p), str(ramen_scripts / p.name))

# Rewrite references to moved scripts in source/config files.
moved_names = [p.name for p in root_python]
text_suffixes = {'.yml', '.yaml', '.toml', '.md', '.txt', '.rs', '.py'}
for p in ROOT.rglob('*'):
    if not p.is_file() or '.git' in p.parts or p == this_script:
        continue
    if p.suffix.lower() not in text_suffixes and p.name != '.gitignore':
        continue
    try:
        old = p.read_text(encoding='utf-8')
    except UnicodeDecodeError:
        continue
    new = old
    for name in moved_names:
        new = new.replace(f'scripts/{name}', f'scripts/ramen/{name}')
    if new != old:
        p.write_text(new, encoding='utf-8')
move(str(this_script), str(ramen_scripts / this_script.name))

# 4. Project notes and snapshots live under .trae.
move('docs/legacy-ai-gap-audit.md', '.trae/documents/legacy-ai-gap-audit.md')
if Path('docs').exists() and not any(Path('docs').iterdir()):
    Path('docs').rmdir()
for p in sorted(Path('exports').glob('*')) if Path('exports').exists() else []:
    if p.is_file():
        move(str(p), str(Path('.trae/exports') / p.name))
if Path('exports').exists() and not any(Path('exports').iterdir()):
    Path('exports').rmdir()

# Generated reports remain usable as ignored Actions artifacts, but are never git-added.
for wf in Path('.github/workflows').glob('*.yml'):
    old = wf.read_text(encoding='utf-8')
    lines = old.splitlines()
    changed = False
    out = []
    for line in lines:
        stripped = line.strip()
        if stripped.startswith('git add ') and ('benchmark-results' in stripped or '$OUT' in stripped or '"$REPORT"' in stripped):
            indent = line[:len(line) - len(line.lstrip())]
            out.append(indent + 'echo "benchmark results are artifact-only; nothing is committed"')
            changed = True
        else:
            out.append(line)
    if changed:
        wf.write_text('\n'.join(out) + '\n', encoding='utf-8')

# Remove the obsolete test repair workflow. Keep the currently executing migration workflow;
# deleting it in the same GITHUB_TOKEN push can be rejected by GitHub workflow protection.
Path('.github/workflows/fix-core-test-logger.yml').unlink(missing_ok=True)
