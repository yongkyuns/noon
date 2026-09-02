#!/usr/bin/env python3
"""Report Noon coverage of the pinned ManimCE public namespace.

This intentionally measures API *presence/classification*, not behavioral parity.
Behavioral equivalence belongs to scripts/manim-differential.py (#57). Tutorial
and example coverage is read from the same manifest used by the executable browser
corpus (#91) when that manifest is present.
"""

from __future__ import annotations

import argparse
import ast
import json
import sys
from collections import Counter, defaultdict
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[1]
PY_ROOT = ROOT / "web" / "python"
TUTORIAL_MANIFEST = PY_ROOT / "examples" / "manim_tutorial_manifest.json"


def _literal_strings(node: ast.AST) -> set[str]:
    try:
        value = ast.literal_eval(node)
    except (ValueError, TypeError):
        return set()
    if isinstance(value, (list, tuple, set)):
        return {item for item in value if isinstance(item, str)}
    return set()


def _dict_string_keys(node: ast.AST) -> set[str]:
    if not isinstance(node, ast.Dict):
        return set()
    result: set[str] = set()
    for key in node.keys:
        if isinstance(key, ast.Constant) and isinstance(key.value, str):
            result.add(key.value)
    return result


def _public_mapping_keys(node: ast.AST) -> set[str]:
    """Recover keys from a literal public mapping or its ``.items()`` view."""

    if isinstance(node, ast.Call) and isinstance(node.func, ast.Attribute):
        if node.func.attr == "items" and not node.args and not node.keywords:
            return _dict_string_keys(node.func.value)
    return _dict_string_keys(node)


def _loop_registers_public_name(node: ast.For) -> bool:
    """Require a mapping loop to append its name into the public export list."""

    if not isinstance(node.target, ast.Tuple):
        return False
    target_names = {item.id for item in node.target.elts if isinstance(item, ast.Name)}
    if not target_names:
        return False
    for child in ast.walk(ast.Module(body=node.body, type_ignores=[])):
        if not isinstance(child, ast.Call) or not isinstance(child.func, ast.Attribute):
            continue
        if child.func.attr != "append" or len(child.args) != 1:
            continue
        receiver = ast.unparse(child.func.value)
        argument = child.args[0]
        if (
            (receiver.endswith(".__all__") or receiver == "__all__")
            and isinstance(argument, ast.Name)
            and argument.id in target_names
        ):
            return True
    return False


def noon_public_exports() -> set[str]:
    """Statically recover the runtime ``noon.__all__`` construction.

    Noon installs compatibility adapters by extending ``noon.__all__`` from small
    ``public = {...}`` dictionaries. Static extraction avoids importing Pyodide-only
    ``js`` bridge modules in this CPython compatibility job.
    """

    exports: set[str] = set()
    sources = [PY_ROOT / "noon.py", *sorted(PY_ROOT.glob("_manim*.py"))]
    for path in sources:
        tree = ast.parse(path.read_text(encoding="utf-8"), filename=str(path))
        for node in ast.walk(tree):
            if isinstance(node, ast.Assign):
                for target in node.targets:
                    if isinstance(target, ast.Name) and target.id == "__all__":
                        exports.update(_literal_strings(node.value))
                    if isinstance(target, ast.Name) and target.id == "public":
                        exports.update(_public_mapping_keys(node.value))
            if isinstance(node, ast.For):
                # Some adapters register a small public mapping directly in a
                # ``for name, value in {...}.items()`` loop rather than assigning
                # it to ``public`` first (notably retained Write/Unwrite).
                target = node.target
                if (
                    isinstance(target, ast.Tuple)
                    and len(target.elts) == 2
                    and all(isinstance(item, ast.Name) for item in target.elts)
                    and _loop_registers_public_name(node)
                ):
                    exports.update(_public_mapping_keys(node.iter))
            if isinstance(node, ast.Call) and isinstance(node.func, ast.Attribute):
                if node.func.attr == "append" and len(node.args) == 1:
                    arg = node.args[0]
                    if isinstance(arg, ast.Constant) and isinstance(arg.value, str):
                        receiver = ast.unparse(node.func.value)
                        if receiver.endswith(".__all__") or receiver == "__all__":
                            exports.add(arg.value)
    return exports


def manim_public_exports(manim: Any) -> list[str]:
    explicit = getattr(manim, "__all__", None)
    if explicit:
        return sorted({name for name in explicit if isinstance(name, str)})
    return sorted(name for name in dir(manim) if not name.startswith("_"))


def module_name(value: Any) -> str:
    return str(getattr(value, "__module__", ""))


def classify(name: str, value: Any, noon_exports: set[str], policy: dict[str, Any]) -> dict[str, Any]:
    override = policy.get("overrides", {}).get(name)
    if override is not None:
        result = dict(override)
    else:
        module = module_name(value)
        result = None
        for rule in policy.get("module_rules", []):
            if module.startswith(rule["prefix"]):
                result = dict(rule)
                result.pop("prefix", None)
                break
        if result is None:
            result = dict(policy["default"])

    result["module"] = module_name(value)
    result["noon_exported"] = name in noon_exports
    if name in noon_exports and name not in policy.get("overrides", {}):
        # Presence without a feature-specific classification is deliberately not
        # promoted to supported. It remains visible as partial until reviewed.
        result["status"] = "partial"
        result["category"] = result.get("category", "unclassified-noon-export")
        result.setdefault("reason", "Exported by Noon but not yet behaviorally classified.")
    return result


def load_tutorial_examples() -> list[dict[str, Any]]:
    if not TUTORIAL_MANIFEST.exists():
        return []
    payload = json.loads(TUTORIAL_MANIFEST.read_text(encoding="utf-8"))
    entries = payload.get("entries", [])
    if not isinstance(entries, list):
        raise ValueError("tutorial manifest entries must be an array")
    return entries


def validate_tutorial_examples(entries: list[dict[str, Any]]) -> list[str]:
    errors: list[str] = []
    allowed_statuses = {"ready", "blocked", "deferred"}
    seen: set[str] = set()
    for index, entry in enumerate(entries):
        prefix = f"tutorial entry {index}"
        if not isinstance(entry, dict):
            errors.append(f"{prefix}: must be an object")
            continue
        entry_id = entry.get("id")
        if not isinstance(entry_id, str) or not entry_id:
            errors.append(f"{prefix}: missing non-empty id")
            continue
        prefix = f"tutorial {entry_id!r}"
        if entry_id in seen:
            errors.append(f"{prefix}: duplicate id")
        seen.add(entry_id)
        status = entry.get("status")
        if status not in allowed_statuses:
            errors.append(f"{prefix}: invalid status {status!r}")
            continue
        if status == "ready":
            relative_path = entry.get("path")
            if not isinstance(relative_path, str) or not relative_path:
                errors.append(f"{prefix}: ready entry requires path")
            elif not (ROOT / "web" / relative_path).is_file():
                errors.append(f"{prefix}: missing fixture web/{relative_path}")
            for field in ("upstream", "reuse"):
                if not entry.get(field):
                    errors.append(f"{prefix}: ready entry requires {field}")
        else:
            dependency = entry.get("dependency")
            if not isinstance(dependency, str) or not dependency.startswith("#"):
                errors.append(f"{prefix}: {status} entry requires issue dependency")
    return errors


def browser_evidence_for(
    name: str, tutorial_examples: list[dict[str, Any]]
) -> list[str]:
    """Return ready tutorial IDs that exercise a public symbol by feature name."""

    return sorted(
        entry["id"]
        for entry in tutorial_examples
        if entry.get("status") == "ready"
        and isinstance(entry.get("id"), str)
        and isinstance(entry.get("features"), list)
        and name in entry["features"]
    )


def validate(
    policy: dict[str, Any],
    manim: Any,
    rows: dict[str, dict[str, Any]],
    tutorial_examples: list[dict[str, Any]] | None = None,
) -> list[str]:
    errors: list[str] = []
    tutorial_examples = tutorial_examples or []
    expected_version = policy["reference"]["version"]
    actual_version = str(getattr(manim, "__version__", ""))
    if actual_version != expected_version:
        errors.append(f"expected ManimCE {expected_version}, found {actual_version}")

    valid_statuses = set(policy["statuses"])
    upstream = set(rows)
    for name, override in policy.get("overrides", {}).items():
        if name not in upstream:
            errors.append(f"manifest override {name!r} is not public in pinned ManimCE")
        if override.get("status") not in valid_statuses:
            errors.append(f"manifest override {name!r} has invalid status {override.get('status')!r}")

    for name, row in rows.items():
        if row["status"] not in valid_statuses:
            errors.append(f"{name}: invalid resolved status {row['status']!r}")
        if row["status"] in {"supported", "partial"} and not row["noon_exported"]:
            errors.append(f"{name}: marked {row['status']} but is absent from Noon's public namespace")
        browser_evidence = browser_evidence_for(name, tutorial_examples)
        row["browser_evidence"] = browser_evidence
        if row["status"] == "supported" and not row.get("evidence"):
            errors.append(f"{name}: supported classifications require executable evidence")
        if row["status"] == "blocked" and row["noon_exported"] and browser_evidence:
            errors.append(
                f"{name}: exported facade has ready browser evidence but remains blocked "
                f"({', '.join(browser_evidence)})"
            )
    return errors


def render_markdown(
    version: str,
    rows: dict[str, dict[str, Any]],
    tutorial_examples: list[dict[str, Any]],
) -> str:
    status_counts = Counter(row["status"] for row in rows.values())
    category_counts: dict[str, Counter[str]] = defaultdict(Counter)
    for row in rows.values():
        category_counts[row["category"]][row["status"]] += 1

    lines = [
        f"# ManimCE {version} public API coverage",
        "",
        f"Total upstream public symbols: **{len(rows)}**",
        "",
        "| Status | Count |",
        "| --- | ---: |",
    ]
    for status in sorted(status_counts):
        lines.append(f"| {status} | {status_counts[status]} |")

    lines.extend([
        "",
        "## By category",
        "",
        "| Category | supported | partial | blocked | deferred | missing | intentional-divergence |",
        "| --- | ---: | ---: | ---: | ---: | ---: | ---: |",
    ])
    for category in sorted(category_counts):
        counts = category_counts[category]
        lines.append(
            f"| {category} | {counts['supported']} | {counts['partial']} | {counts['blocked']} | "
            f"{counts['deferred']} | {counts['missing']} | {counts['intentional-divergence']} |"
        )

    lines.extend(["", "## Tutorial/example coverage", ""])
    if tutorial_examples:
        example_counts = Counter(entry["status"] for entry in tutorial_examples)
        lines.extend([
            f"Tracked tutorial/example entries: **{len(tutorial_examples)}**",
            "",
            "| Status | Count |",
            "| --- | ---: |",
        ])
        for status in ("ready", "blocked", "deferred"):
            lines.append(f"| {status} | {example_counts[status]} |")
        lines.extend(["", "Unready tutorial/example entries:", ""])
        for entry in tutorial_examples:
            if entry["status"] == "ready":
                continue
            lines.append(
                f"- `{entry['id']}` — **{entry['status']}** — {entry.get('dependency', 'unclassified')}"
            )
    else:
        lines.append("No executable tutorial manifest is present on this ref yet.")

    lines.extend(["", "## Missing / blocked / deferred symbols", ""])
    for name in sorted(rows):
        row = rows[name]
        if row["status"] == "supported":
            continue
        dependency = row.get("dependency", "")
        reason = row.get("reason", "")
        suffix = " · ".join(part for part in (dependency, reason) if part)
        lines.append(f"- `{name}` — **{row['status']}** / {row['category']}" + (f" — {suffix}" if suffix else ""))
    return "\n".join(lines) + "\n"


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--manifest", default="compat/manim-v0.21.0.json")
    parser.add_argument("--json", action="store_true")
    parser.add_argument("--check", action="store_true")
    parser.add_argument("--write-markdown")
    args = parser.parse_args()

    manifest_path = ROOT / args.manifest
    policy = json.loads(manifest_path.read_text(encoding="utf-8"))

    try:
        import manim
    except ImportError as exc:
        raise SystemExit("Install pinned ManimCE before running this report") from exc

    noon_exports = noon_public_exports()
    names = manim_public_exports(manim)
    rows = {name: classify(name, getattr(manim, name), noon_exports, policy) for name in names}
    tutorial_examples = load_tutorial_examples()
    errors = validate(policy, manim, rows, tutorial_examples)
    errors.extend(validate_tutorial_examples(tutorial_examples))

    payload = {
        "manim_version": manim.__version__,
        "noon_public_exports": sorted(noon_exports),
        "symbols": rows,
        "tutorial_examples": tutorial_examples,
        "errors": errors,
    }
    if args.json:
        print(json.dumps(payload, indent=2, sort_keys=True))
    else:
        print(render_markdown(str(manim.__version__), rows, tutorial_examples))

    if args.write_markdown:
        output = ROOT / args.write_markdown
        output.parent.mkdir(parents=True, exist_ok=True)
        output.write_text(
            render_markdown(str(manim.__version__), rows, tutorial_examples),
            encoding="utf-8",
        )

    if errors:
        for error in errors:
            print(f"coverage error: {error}", file=sys.stderr)
    return 1 if args.check and errors else 0


if __name__ == "__main__":
    raise SystemExit(main())
