#!/usr/bin/env python3
"""Check pending reference ownership and stale namespace blockers, not parity."""
from __future__ import annotations

import argparse
import importlib.util
import json
from collections import Counter
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def validate_namespace_blockers(dispositions, exports):
    errors = []
    for entry in dispositions.get("entries", []):
        names = entry.get("namespace_blockers", [])
        if not isinstance(names, list) or any(not isinstance(name, str) or not name for name in names):
            errors.append(f"{entry.get('id')}: invalid namespace blockers")
            continue
        if entry.get("blocker_kind") == "namespace":
            if entry.get("status") != "blocked" or not names:
                errors.append(f"{entry.get('id')}: namespace blocker requires blocked status and names")
            for name in names:
                if name in exports:
                    errors.append(f"{entry.get('id')}: stale namespace blocker {name} is now exported; reconcile its disposition without assuming parity")
        elif names:
            errors.append(f"{entry.get('id')}: namespace names require namespace blocker kind")
    return errors


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--dispositions", type=Path, default=ROOT / "compat/manim-v0.21.0-reference-dispositions.json")
    args = parser.parse_args()
    spec = importlib.util.spec_from_file_location("coverage", ROOT / "scripts/manim-api-coverage.py")
    coverage = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(coverage)
    dispositions = json.loads(args.dispositions.read_text())
    errors = validate_namespace_blockers(dispositions, coverage.noon_public_exports())
    print(json.dumps({"pending_examples": len(dispositions["entries"]),
                      "statuses": dict(Counter(e["status"] for e in dispositions["entries"])),
                      "errors": errors}, indent=2))
    return bool(errors)


if __name__ == "__main__":
    raise SystemExit(main())
