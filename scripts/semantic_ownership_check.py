#!/usr/bin/env python3
"""Validate the machine-readable semantic ownership inventory for issue #61."""

from __future__ import annotations

import argparse
import json
import re
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[1]
DEFAULT_INVENTORY = ROOT / "compat/semantic-ownership-v1.json"
CLASSIFICATIONS = {
    "shared-rust",
    "python-adapter-only",
    "python-semantic-duplicate",
    "host-language-required",
}
REQUIRED_DUPLICATE_FIELDS = {"reason", "replacement", "migration_issue"}


def _location(value: Any, label: str, errors: list[str]) -> None:
    if not isinstance(value, dict):
        errors.append(f"{label} must be an object")
        return
    for field in ("language", "path", "symbol"):
        if not isinstance(value.get(field), str) or not value[field].strip():
            errors.append(f"{label}.{field} must be a non-empty string")
    path = value.get("path")
    if isinstance(path, str):
        candidate = (ROOT / path).resolve()
        if ROOT not in candidate.parents or not candidate.is_file():
            errors.append(f"{label}.path does not name a repository file: {path}")
            return
        symbol = value.get("symbol")
        if isinstance(symbol, str):
            source = candidate.read_text(encoding="utf-8")
            anchors = re.findall(r"[A-Za-z_][A-Za-z0-9_]*", symbol)
            missing = sorted({anchor for anchor in anchors if anchor not in source})
            if missing:
                errors.append(
                    f"{label}.symbol has anchor(s) absent from {path}: "
                    f"{', '.join(missing)}"
                )


def validate_inventory(document: Any) -> list[str]:
    """Return deterministic validation errors; an empty list means valid."""

    errors: list[str] = []
    if not isinstance(document, dict):
        return ["inventory root must be an object"]
    if document.get("schema_version") != 1:
        errors.append("schema_version must be 1")
    if document.get("issue") != "#61":
        errors.append("issue must be #61")
    declared = document.get("classifications")
    if declared != sorted(CLASSIFICATIONS):
        errors.append("classifications must declare the four supported ownership classes")

    budgets = document.get("duplicate_debt_budgets")
    if not isinstance(budgets, list) or not budgets:
        errors.append("duplicate_debt_budgets must be a non-empty list")
    else:
        seen_budgets: set[tuple[str, str]] = set()
        for index, budget in enumerate(budgets):
            label = f"duplicate_debt_budgets[{index}]"
            if not isinstance(budget, dict):
                errors.append(f"{label} must be an object")
                continue
            path = budget.get("path")
            token = budget.get("token")
            maximum = budget.get("maximum")
            if not isinstance(path, str) or not path.startswith("web/python/"):
                errors.append(f"{label}.path must name a web/python source file")
                continue
            if not isinstance(token, str) or not token:
                errors.append(f"{label}.token must be a non-empty string")
                continue
            key = (path, token)
            if key in seen_budgets:
                errors.append(f"duplicate semantic-debt budget: {path} / {token}")
            seen_budgets.add(key)
            if not isinstance(maximum, int) or maximum < 0:
                errors.append(f"{label}.maximum must be a non-negative integer")
                continue
            candidate = (ROOT / path).resolve()
            if ROOT not in candidate.parents or not candidate.is_file():
                errors.append(f"{label}.path does not name a repository file: {path}")
                continue
            actual = candidate.read_text(encoding="utf-8").count(token)
            if actual > maximum:
                errors.append(
                    f"{label} semantic debt increased for {path}: "
                    f"{token!r} occurs {actual} times (maximum {maximum})"
                )
            elif actual < maximum:
                errors.append(
                    f"{label} semantic-debt budget is stale for {path}: "
                    f"{token!r} occurs {actual} times (budget {maximum})"
                )
            if budget.get("migration_issue") != "#61":
                errors.append(f"{label}.migration_issue must be #61")

    operations = document.get("operations")
    if not isinstance(operations, list) or not operations:
        return [*errors, "operations must be a non-empty list"]

    seen: set[str] = set()
    for index, operation in enumerate(operations):
        label = f"operations[{index}]"
        if not isinstance(operation, dict):
            errors.append(f"{label} must be an object")
            continue
        operation_id = operation.get("id")
        if not isinstance(operation_id, str) or not re.fullmatch(
            r"[a-z0-9][a-z0-9_.-]*", operation_id
        ):
            errors.append(f"{label}.id must be a stable lowercase identifier")
        elif operation_id in seen:
            errors.append(f"duplicate operation id: {operation_id}")
        else:
            seen.add(operation_id)
        if not isinstance(operation.get("surface"), str) or not operation["surface"].strip():
            errors.append(f"{label}.surface must be a non-empty string")

        classification = operation.get("classification")
        if classification not in CLASSIFICATIONS:
            errors.append(f"{label}.classification is unknown: {classification!r}")
        _location(operation.get("owner"), f"{label}.owner", errors)
        owner = operation.get("owner")
        owner_language = owner.get("language") if isinstance(owner, dict) else None
        if classification == "shared-rust" and owner_language != "rust":
            errors.append(f"{label} shared-rust entries must be Rust-owned")
        if (
            classification
            in {
                "python-adapter-only",
                "python-semantic-duplicate",
                "host-language-required",
            }
            and owner_language != "python"
        ):
            errors.append(f"{label} {classification} entries must be Python-owned")

        adapters = operation.get("adapters", [])
        if not isinstance(adapters, list):
            errors.append(f"{label}.adapters must be a list")
        else:
            for adapter_index, adapter in enumerate(adapters):
                _location(adapter, f"{label}.adapters[{adapter_index}]", errors)

        if classification == "python-semantic-duplicate":
            missing = sorted(
                field
                for field in REQUIRED_DUPLICATE_FIELDS
                if not operation.get(field)
            )
            if missing:
                errors.append(
                    f"{label} unexplained python-semantic-duplicate; "
                    f"missing {', '.join(missing)}"
                )
            shared_owner = operation.get("shared_owner")
            _location(shared_owner, f"{label}.shared_owner", errors)
            shared_owner_language = (
                shared_owner.get("language") if isinstance(shared_owner, dict) else None
            )
            if shared_owner_language != "rust":
                errors.append(
                    f"{label} python-semantic-duplicate shared_owner must be Rust-owned"
                )
            if operation.get("migration_issue") != "#61":
                errors.append(f"{label}.migration_issue must be #61")
        if (
            classification == "host-language-required"
            and operation.get("host_callback") is not True
        ):
            errors.append(f"{label} host-language-required entries must set host_callback=true")
    return errors


def load_inventory(path: Path = DEFAULT_INVENTORY) -> dict[str, Any]:
    with path.open(encoding="utf-8") as handle:
        document = json.load(handle)
    errors = validate_inventory(document)
    if errors:
        raise ValueError("semantic ownership inventory is invalid:\n- " + "\n- ".join(errors))
    return document


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--inventory", type=Path, default=DEFAULT_INVENTORY)
    args = parser.parse_args()
    try:
        document = load_inventory(args.inventory.resolve())
    except (OSError, json.JSONDecodeError, ValueError) as error:
        print(f"semantic ownership inventory: FAIL\n{error}")
        return 1
    operations = document["operations"]
    counts = {
        name: sum(item["classification"] == name for item in operations)
        for name in sorted(CLASSIFICATIONS)
    }
    summary = ", ".join(f"{name}={counts[name]}" for name in sorted(counts))
    print(f"semantic ownership inventory: OK ({len(operations)} operations; {summary})")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
