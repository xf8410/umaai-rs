from pathlib import Path
import shutil


def move(src: str, dst: str) -> None:
    source, target = Path(src), Path(dst)
    if not source.exists():
        return
    target.parent.mkdir(parents=True, exist_ok=True)
    if target.exists():
        raise SystemExit(f"destination already exists: {target}")
    shutil.move(str(source), str(target))


# Generated benchmark outputs are artifacts, not repository source.
shutil.rmtree("benchmark-results", ignore_errors=True)
gitignore = Path(".gitignore")
text = gitignore.read_text(encoding="utf-8") if gitignore.exists() else ""
if "/benchmark-results/" not in text.splitlines():
    text = text.rstrip() + "\n\n# Generated benchmark and training reports\n/benchmark-results/\n"
gitignore.write_text(text, encoding="utf-8")

# Data collection and label generation are tools, not ordinary user-facing binaries.
data_tools = [
    "ramen_feature_dataset",
    "ramen_low_score_diagnostic",
    "skill_pt_matrix",
    "skill_pt_phase_matrix",
    "y3_train_pt_trace",
]
for name in data_tools:
    move(f"crates/umasim/src/bin/{name}.rs", f"crates/umasim/tools/data_collection/{name}.rs")

cargo = Path("crates/umasim/Cargo.toml")
cargo_text = cargo.read_text(encoding="utf-8")
marker = "# Data-collection and label-generation tools are kept outside src/bin."
if marker not in cargo_text:
    blocks = ["\n" + marker]
    for name in data_tools:
        blocks.append(f'\n[[bin]]\nname = "{name}"\npath = "tools/data_collection/{name}.rs"\n')
    anchor = cargo_text.find("\n[features]")
    if anchor < 0:
        raise SystemExit("Cargo.toml [features] anchor missing")
    cargo_text = cargo_text[:anchor] + "".join(blocks) + cargo_text[anchor:]
cargo.write_text(cargo_text, encoding="utf-8")

# Keep project-specific Python under a dedicated namespace. Workflow references are updated
# separately through reviewed GitHub file writes so the Actions token never pushes workflow
# mutations together with this source-layout commit.
script_dir = Path("scripts")
ramen_scripts = script_dir / "ramen"
ramen_scripts.mkdir(parents=True, exist_ok=True)
this_script = Path(__file__)
for script in sorted(p for p in script_dir.glob("*.py") if p != this_script):
    move(str(script), str(ramen_scripts / script.name))
move(str(this_script), str(ramen_scripts / this_script.name))

# Internal project documents and snapshots belong under .trae.
move("docs/legacy-ai-gap-audit.md", ".trae/documents/legacy-ai-gap-audit.md")
if Path("docs").exists() and not any(Path("docs").iterdir()):
    Path("docs").rmdir()
for exported in sorted(Path("exports").glob("*")) if Path("exports").exists() else []:
    if exported.is_file():
        move(str(exported), str(Path(".trae/exports") / exported.name))
if Path("exports").exists() and not any(Path("exports").iterdir()):
    Path("exports").rmdir()
