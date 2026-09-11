#!/usr/bin/env python3
"""Validate the repository-bound Noon skill and its maintained example references."""
from __future__ import annotations

import importlib.util
import json
import re
import sys
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[1]
SKILL = "skills/noon-authoring"
COMMANDS = (
    "scripts/noon-capabilities.py", "scripts/build-web-demo.sh",
    "scripts/check.sh", "scripts/manim-tutorial-smoke.mjs",
    "tools/noon-mcp/bin/noon-preview.mjs",
    "tools/noon-mcp/src/server.mjs",
    "tools/noon-mcp/scripts/setup-preview-runtime.mjs",
    "tools/noon-mcp/scripts/package-manifest.mjs",
)


def validate_skill(root: Path, report: dict[str, Any]) -> int:
    directory = root / SKILL
    text = (directory / "SKILL.md").read_text(encoding="utf-8")
    match = re.match(r"\A---\n(.*?)\n---\n", text, re.DOTALL)
    if not match:
        raise ValueError("SKILL.md requires YAML frontmatter")
    fields = dict(re.findall(r"^(name|description|compatibility): (.+)$", match[1], re.MULTILINE))
    if fields.get("name") != directory.name:
        raise ValueError("skill name must match noon-authoring directory")
    if not 1 <= len(fields.get("description", "")) <= 1024:
        raise ValueError("skill description must be a non-empty one-line scalar <= 1024 characters")
    if not 1 <= len(fields.get("compatibility", "")) <= 500:
        raise ValueError("skill compatibility must state requirements in <= 500 characters")
    for path in sorted(directory.rglob("*.md")):
        for target in re.findall(r"\[[^\]]*\]\(([^)]+)\)", path.read_text(encoding="utf-8")):
            if "://" in target or target.startswith("#"):
                continue
            resolved = (path.parent / target.split("#", 1)[0]).resolve()
            if not resolved.is_relative_to(root.resolve()) or not resolved.is_file():
                raise ValueError(f"{path.name}: missing or unconfined link {target!r}")
    for command in COMMANDS:
        if not (root / command).is_file():
            raise ValueError(f"missing documented command: {command}")
    examples = (directory / "references/examples.md").read_text(encoding="utf-8")
    ids = re.findall(r"^\| `([^`]+)` \|", examples, re.MULTILINE)
    if not ids or len(ids) != len(set(ids)):
        raise ValueError("example table must contain unique example IDs")
    for example_id in ids:
        record = report["examples"].get(example_id)
        if not record or record.get("status") != "ready":
            raise ValueError(f"skill example is missing or not ready: {example_id}")
        relative = record.get("repository_path")
        if not isinstance(relative, str):
            raise ValueError(f"skill example has no source: {example_id}")
        resolved = (root / relative).resolve()
        if not resolved.is_relative_to(root.resolve()) or not resolved.is_file():
            raise ValueError(f"skill example source is missing or unconfined: {example_id}")
    return len(ids)


def main() -> int:
    try:
        spec = importlib.util.spec_from_file_location("noon_capabilities", ROOT / COMMANDS[0])
        if spec is None or spec.loader is None:
            raise ValueError("cannot load capability exporter")
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        count = validate_skill(ROOT, module.build_report(ROOT))
    except (OSError, ValueError, TypeError, KeyError, SyntaxError) as exc:
        print(json.dumps({"ok": False, "error": str(exc)}), file=sys.stderr)
        return 1
    print(f"Noon skill metadata, links, commands and {count} ready example references validated (no scenes executed).")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
