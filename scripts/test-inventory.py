#!/usr/bin/env python3
"""Generate a machine-readable inventory of Noon's production and test surface."""

from __future__ import annotations

import argparse
import json
from collections import Counter, defaultdict
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]

RUST_SOURCE = "crates/*/src/**/*.rs"
RUST_INTEGRATION = "crates/*/tests/**/*.rs"
JS_SOURCE = "web/*.js"
JS_TEST = "web/*.test.mjs"
PY_SOURCE = "web/python/*.py"
PY_TEST = "web/python/test_*.py"
BROWSER_TEST = "scripts/*smoke.mjs"

GENERATED_OR_SUPPORT = {
    "web/browser-smoke.js",  # harness loaded by scripts/browser-smoke.mjs
}


def rel(path: Path) -> str:
    return path.relative_to(ROOT).as_posix()


def crate_name(path: Path) -> str:
    parts = path.relative_to(ROOT).parts
    return parts[1] if len(parts) > 1 and parts[0] == "crates" else "unknown"


def classify_test(path: Path) -> str:
    name = path.name.lower()
    text = rel(path)
    if "manim-differential" in text:
        return "differential"
    if "perf" in name or "profile" in name:
        return "performance"
    if "smoke" in name:
        return "browser-smoke"
    if "property" in name or "fuzz" in name or "stress" in name:
        return "property-stress"
    if path.suffix == ".rs" and "/tests/" in f"/{text}":
        return "integration"
    return "unit"


def production_layer(path: Path) -> str:
    text = rel(path)
    if text.startswith("crates/"):
        return crate_name(path)
    if text.startswith("web/python/"):
        return "web-python"
    if text.startswith("web/"):
        return "web-js"
    return "other"


def source_strategy(
    path: Path,
    rust_tests_by_crate: dict[str, list[str]],
    rust_inline_by_crate: set[str],
    js_tests: set[str],
    python_tests: list[str],
) -> list[str]:
    text = rel(path)
    strategies: list[str] = []
    if path.suffix == ".rs":
        contents = path.read_text(encoding="utf-8")
        crate = crate_name(path)
        if "#[cfg(test)]" in contents:
            strategies.append("inline-unit")
        if crate in rust_inline_by_crate:
            strategies.append("crate-unit")
        if rust_tests_by_crate.get(crate):
            strategies.append("crate-integration")
    elif text.startswith("web/python/"):
        module = path.stem
        exact = f"web/python/test_{module.lstrip('_')}.py"
        if exact in python_tests:
            strategies.append("module-unit")
        if python_tests:
            strategies.append("python-suite")
    elif text.startswith("web/"):
        exact = path.with_suffix(".test.mjs").name
        if f"web/{exact}" in js_tests:
            strategies.append("module-unit")
        if text in GENERATED_OR_SUPPORT:
            strategies.append("browser-harness")
        strategies.append("browser-smoke")
    return sorted(set(strategies))


def collect() -> dict[str, object]:
    rust_sources = sorted(ROOT.glob(RUST_SOURCE))
    rust_tests = sorted(ROOT.glob(RUST_INTEGRATION))
    js_sources = sorted(ROOT.glob(JS_SOURCE))
    js_tests_paths = sorted(ROOT.glob(JS_TEST))
    python_sources = sorted(
        path
        for path in ROOT.glob(PY_SOURCE)
        if not path.name.startswith("test_") and path.name != "playground_examples.py"
    )
    python_tests_paths = sorted(ROOT.glob(PY_TEST))
    browser_tests = sorted(ROOT.glob(BROWSER_TEST))

    special_tests = [
        ROOT / "scripts/manim-differential.py",
        ROOT / "scripts/manim-api-coverage.py",
    ]
    all_tests = [*rust_tests, *js_tests_paths, *python_tests_paths, *browser_tests]
    all_tests.extend(path for path in special_tests if path.exists())

    rust_tests_by_crate: dict[str, list[str]] = defaultdict(list)
    for path in rust_tests:
        rust_tests_by_crate[crate_name(path)].append(rel(path))
    rust_inline_by_crate = {
        crate_name(path)
        for path in rust_sources
        if "#[cfg(test)]" in path.read_text(encoding="utf-8")
    }
    js_test_names = {rel(path) for path in js_tests_paths}
    python_test_names = [rel(path) for path in python_tests_paths]

    production = [*rust_sources, *js_sources, *python_sources]
    modules = []
    uncovered = []
    for path in production:
        strategies = source_strategy(
            path,
            rust_tests_by_crate,
            rust_inline_by_crate,
            js_test_names,
            python_test_names,
        )
        entry = {
            "path": rel(path),
            "layer": production_layer(path),
            "strategies": strategies,
        }
        modules.append(entry)
        if not strategies:
            uncovered.append(entry["path"])

    tests = [
        {
            "path": rel(path),
            "layer": production_layer(path)
            if not rel(path).startswith("scripts/")
            else "browser/compat",
            "type": classify_test(path),
        }
        for path in all_tests
    ]

    return {
        "schema_version": 1,
        "production_modules": modules,
        "tests": tests,
        "summary": {
            "production_modules": len(modules),
            "tests": len(tests),
            "production_by_layer": dict(
                sorted(Counter(item["layer"] for item in modules).items())
            ),
            "tests_by_type": dict(
                sorted(Counter(item["type"] for item in tests).items())
            ),
            "uncovered_modules": uncovered,
        },
    }


def markdown(report: dict[str, object]) -> str:
    summary = report["summary"]
    lines = [
        "# Noon test inventory",
        "",
        "> Generated by `scripts/test-inventory.py`. The inventory describes test ownership/strategy; line coverage is emitted separately by the coverage workflow.",
        "",
        f"- Production modules: **{summary['production_modules']}**",
        f"- Test entry points: **{summary['tests']}**",
        f"- Modules without an explicit test strategy: **{len(summary['uncovered_modules'])}**",
        "",
        "## Production layers",
        "",
        "| Layer | Modules |",
        "|---|---:|",
    ]
    for layer, count in summary["production_by_layer"].items():
        lines.append(f"| `{layer}` | {count} |")
    lines.extend(["", "## Test types", "", "| Type | Entry points |", "|---|---:|"])
    for kind, count in summary["tests_by_type"].items():
        lines.append(f"| `{kind}` | {count} |")
    if summary["uncovered_modules"]:
        lines.extend(["", "## Modules without an explicit strategy", ""])
        lines.extend(f"- `{path}`" for path in summary["uncovered_modules"])
    lines.extend(["", "## Test entry points", ""])
    for item in report["tests"]:
        lines.append(f"- `{item['path']}` — {item['type']}")
    lines.append("")
    return "\n".join(lines)


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--json", type=Path)
    parser.add_argument("--markdown", type=Path)
    parser.add_argument(
        "--check",
        action="store_true",
        help="fail when a production module has no explicit test strategy",
    )
    args = parser.parse_args()

    report = collect()
    if args.json:
        args.json.parent.mkdir(parents=True, exist_ok=True)
        args.json.write_text(
            json.dumps(report, indent=2, sort_keys=True) + "\n", encoding="utf-8"
        )
    if args.markdown:
        args.markdown.parent.mkdir(parents=True, exist_ok=True)
        args.markdown.write_text(markdown(report), encoding="utf-8")

    summary = report["summary"]
    print(
        f"test inventory: {summary['production_modules']} production modules, "
        f"{summary['tests']} test entry points, "
        f"{len(summary['uncovered_modules'])} modules without explicit strategy"
    )
    if args.check and summary["uncovered_modules"]:
        for path in summary["uncovered_modules"]:
            print(f"uncovered: {path}")
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
