#!/usr/bin/env python3
"""Fail closed when a matrix job is not running the requested immutable source.

The workflow passes SOURCE_SHA from the workflow event. Every matrix and summary job
must verify the same SHA before compiling or reading reports. PATCH_LIST documents
intentional runtime experiment injections; it does not silently select old code.
"""
import json
import os
import subprocess
from pathlib import Path


def git(*args):
    return subprocess.check_output(["git", *args], text=True).strip()

expected = os.environ.get("SOURCE_SHA", "").strip()
if not expected or len(expected) != 40 or any(c not in "0123456789abcdefABCDEF" for c in expected):
    raise SystemExit("SOURCE_SHA must be the exact 40-character workflow source SHA")
actual = git("rev-parse", "HEAD")
if actual.lower() != expected.lower():
    raise SystemExit(f"SOURCE_SHA_MISMATCH: expected {expected}, checked out {actual}")

patches = [p for p in os.environ.get("PATCH_LIST", "").split(",") if p]
for patch in patches:
    if not Path(patch).is_file():
        raise SystemExit(f"DECLARED_PATCH_NOT_FOUND: {patch}")

manifest = {
    "repository": os.environ.get("GITHUB_REPOSITORY", ""),
    "workflow": os.environ.get("GITHUB_WORKFLOW", ""),
    "run_id": os.environ.get("GITHUB_RUN_ID", ""),
    "source_ref": os.environ.get("SOURCE_REF", os.environ.get("GITHUB_REF", "")),
    "source_sha": actual,
    "workflow_sha": os.environ.get("GITHUB_SHA", ""),
    "patches_declared": patches,
    "patches_applied": False,
}
Path("experiment-source-manifest.json").write_text(json.dumps(manifest, ensure_ascii=False, indent=2) + "\n")
print(json.dumps(manifest, ensure_ascii=False))
