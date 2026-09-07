#!/usr/bin/env python3
"""Export a conservative Noon source inventory; never import Manim or execute scenes."""
from __future__ import annotations

import argparse
import hashlib
import importlib.util
import json
import subprocess
import sys
from pathlib import Path, PurePosixPath
from typing import Any

ROOT = Path(__file__).resolve().parents[1]
POLICY = "compat/manim-v0.21.0.json"
TUTORIALS = "web/python/examples/manim_tutorial_manifest.json"
SCHEMA_VERSION = 1


def read_object(path: Path) -> dict[str, Any]:
    value = json.loads(path.read_text(encoding="utf-8"))
    if not isinstance(value, dict):
        raise ValueError(f"{path.name}: expected a JSON object")
    return value


def local_file(root: Path, relative: str) -> Path:
    """Accept only explicit, confined repository paths (including symlink targets)."""
    if not isinstance(relative, str) or not relative or "\\" in relative:
        raise ValueError(f"invalid repository path: {relative!r}")
    path = PurePosixPath(relative)
    if path.is_absolute() or ".." in path.parts:
        raise ValueError(f"unsafe repository path: {relative!r}")
    resolved = (root / relative).resolve()
    if not resolved.is_relative_to(root.resolve()) or not resolved.is_file():
        raise ValueError(f"missing or unconfined repository file: {relative!r}")
    return resolved


def coverage_module(root: Path) -> Any:
    # Reuse the coverage job's static export discovery; do not import noon.py.
    path = local_file(root, "scripts/manim-api-coverage.py")
    spec = importlib.util.spec_from_file_location("noon_coverage_inventory", path)
    if spec is None or spec.loader is None:
        raise ValueError("cannot load the repository coverage helper")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def source_revision(root: Path) -> dict[str, Any]:
    def git(*args: str) -> str:
        return subprocess.check_output(
            ["git", "-C", str(root), *args], stderr=subprocess.DEVNULL,
            text=True, timeout=5,
        ).strip()

    try:
        # Never attribute a source archive to an unrelated parent checkout.
        if Path(git("rev-parse", "--show-toplevel")).resolve() != root.resolve():
            return {"revision": None, "dirty": None}
        return {
            "revision": git("rev-parse", "HEAD"),
            "dirty": bool(git("status", "--porcelain", "--untracked-files=normal")),
        }
    except (OSError, subprocess.SubprocessError):
        return {"revision": None, "dirty": None}


def build_report(root: Path = ROOT) -> dict[str, Any]:
    root = root.resolve()
    policy = read_object(local_file(root, POLICY))
    tutorial = read_object(local_file(root, TUTORIALS))
    reference = policy.get("reference")
    if not isinstance(reference, dict) or reference.get("package") != "manim":
        raise ValueError("compatibility reference must identify the manim package")
    version = reference.get("version")
    if not isinstance(version, str) or not version:
        raise ValueError("compatibility reference requires a version")
    if not isinstance(tutorial.get("reference"), dict) or tutorial["reference"].get("version") != version:
        raise ValueError("tutorial and compatibility reference versions differ")
    statuses = policy.get("statuses")
    if not isinstance(statuses, list) or not statuses or not all(isinstance(s, str) for s in statuses):
        raise ValueError("compatibility statuses must be a non-empty string array")
    if len(set(statuses)) != len(statuses):
        raise ValueError("duplicate compatibility status")
    overrides = policy.get("overrides")
    if not isinstance(overrides, dict):
        raise ValueError("compatibility overrides must be an object")
    entries = tutorial.get("entries")
    if not isinstance(entries, list):
        raise ValueError("tutorial entries must be an array")

    inputs = {POLICY, TUTORIALS, "scripts/manim-api-coverage.py", "scripts/noon-capabilities.py", "web/python/noon.py"}
    inputs.update(path.relative_to(root).as_posix() for path in (root / "web/python").glob("_manim*.py"))
    for name in inputs:
        local_file(root, name)
    coverage = coverage_module(root)
    errors = coverage.validate_tutorial_examples(entries)
    if errors:
        raise ValueError("; ".join(errors))
    exports = coverage.noon_public_exports()
    examples: dict[str, Any] = {}
    for entry in entries:
        entry_id = entry["id"]
        features = entry.get("features", [])
        if not isinstance(features, list) or not all(isinstance(f, str) for f in features):
            raise ValueError(f"{entry_id}: features must be a string array")
        parity = entry.get("parity_status")
        if parity not in (None, "candidate", "parity-qualified"):
            raise ValueError(f"{entry_id}: invalid parity_status")
        if parity == "parity-qualified" and not entry.get("parity_fixture"):
            raise ValueError(f"{entry_id}: parity-qualified requires a parity fixture")
        row = dict(entry)
        row["runtime_verified"] = False
        if entry["status"] == "ready":
            # Manifest paths are relative to web/, never to the current directory.
            relative = "web/" + entry["path"]
            if PurePosixPath(entry["path"]).is_absolute():
                raise ValueError(f"{entry_id}: unsafe absolute example path")
            source = local_file(root, relative)
            inputs.add(relative)
            row["repository_path"] = relative
            row["source_sha256"] = hashlib.sha256(source.read_bytes()).hexdigest()
            upstream = entry.get("upstream_source")
            if upstream is not None:
                local_file(root, upstream)
                inputs.add(upstream)
        examples[entry_id] = row

    symbols: dict[str, Any] = {}
    for name in sorted(set(overrides) | exports):
        explicit = name in overrides
        row = dict(overrides[name]) if isinstance(overrides.get(name), dict) else {}
        if explicit and not row:
            raise ValueError(f"{name}: override must be a non-empty object")
        if explicit and row.get("status") not in statuses:
            raise ValueError(f"{name}: invalid policy status {row.get('status')!r}")
        if not explicit:
            # Upstream module rules need Manim introspection. Do not guess them.
            row = {"status": "partial", "reason": "Statically exported; no explicit per-symbol behavioral classification."}
        present = name in exports
        if row["status"] in {"supported", "partial"} and not present:
            raise ValueError(f"{name}: policy claims {row['status']} but export is absent")
        if row["status"] == "supported" and not row.get("evidence"):
            raise ValueError(f"{name}: supported policy requires declared evidence")
        evidence_ids = coverage.browser_evidence_for(name, entries)
        if row["status"] == "blocked" and present and evidence_ids:
            raise ValueError(f"{name}: blocked export has ready tutorial evidence")
        symbols[name] = {
            "policy": row,
            "classification_source": "override" if explicit else "unclassified-export",
            "exported": present,
            "export_detection": "static",
            "ready_examples": evidence_ids,
            "runtime_verified": False,
        }

    return {
        "schema_version": SCHEMA_VERSION,
        "kind": "noon-agent-capabilities",
        "scope": "source-inventory",
        "reference": reference,
        "provenance": {
            **source_revision(root),
            "input_sha256": {
                name: hashlib.sha256(local_file(root, name).read_bytes()).hexdigest()
                for name in sorted(inputs)
            },
        },
        "qualification": {
            "behavioral_tests_run": False,
            "upstream_module_rules_applied": False,
            "note": "Policy and ready/parity labels are declarations, not proof of this build or session. Read restrictions and validate rendered behavior.",
        },
        "runtime": {"host": None, "renderer_backend": None, "session_capabilities": None},
        "symbols": symbols,
        "examples": dict(sorted(examples.items())),
    }


def select_report(report: dict[str, Any], symbols: list[str], examples: list[str]) -> dict[str, Any]:
    # Validate the whole inventory before narrowing output; filters cannot hide drift.
    for kind, names in (("symbols", symbols), ("examples", examples)):
        unknown = sorted(set(names) - report[kind].keys())
        if unknown:
            raise ValueError(f"unknown {kind}: {', '.join(unknown)}")
    selected = dict(report)
    if symbols:
        selected["symbols"] = {name: report["symbols"][name] for name in sorted(set(symbols))}
    if examples:
        selected["examples"] = {name: report["examples"][name] for name in sorted(set(examples))}
    elif symbols:
        ids = {item for name in symbols for item in report["symbols"][name]["ready_examples"]}
        selected["examples"] = {name: report["examples"][name] for name in sorted(ids)}
    return selected


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--symbol", action="append", default=[], help="Restrict symbols (repeatable). Unknown names fail.")
    parser.add_argument("--example", action="append", default=[], help="Restrict example IDs (repeatable), including blocked entries.")
    args = parser.parse_args(argv)
    try:
        report = select_report(build_report(), args.symbol, args.example)
    except (OSError, ValueError, SyntaxError, TypeError, KeyError) as exc:
        print(json.dumps({"ok": False, "error": str(exc)}, sort_keys=True), file=sys.stderr)
        return 1
    print(json.dumps(report, indent=2, sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
