from __future__ import annotations

import importlib.util
import json
import tempfile
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
CHECKER_PATH = ROOT / "scripts/semantic_ownership_check.py"
INVENTORY_PATH = ROOT / "compat/semantic-ownership-v1.json"
SPEC = importlib.util.spec_from_file_location("semantic_ownership_check", CHECKER_PATH)
assert SPEC and SPEC.loader
CHECKER = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(CHECKER)


class SemanticOwnershipInventoryTests(unittest.TestCase):
    def test_checked_in_inventory_has_valid_locations_and_explanations(self) -> None:
        document = CHECKER.load_inventory(INVENTORY_PATH)
        self.assertGreaterEqual(len(document["operations"]), 12)
        self.assertTrue(
            any(
                item["classification"] == "python-semantic-duplicate"
                for item in document["operations"]
            )
        )

    def test_unexplained_python_duplicate_is_rejected(self) -> None:
        with INVENTORY_PATH.open(encoding="utf-8") as handle:
            document = json.load(handle)
        duplicate = next(
            item
            for item in document["operations"]
            if item["classification"] == "python-semantic-duplicate"
        )
        duplicate.pop("reason")
        errors = CHECKER.validate_inventory(document)
        self.assertTrue(any("unexplained python-semantic-duplicate" in error for error in errors))

    def test_missing_owner_file_is_rejected(self) -> None:
        with INVENTORY_PATH.open(encoding="utf-8") as handle:
            document = json.load(handle)
        document["operations"][0]["owner"]["path"] = "compat/does-not-exist.rs"
        errors = CHECKER.validate_inventory(document)
        self.assertTrue(any("does not name a repository file" in error for error in errors))

    def test_missing_owner_symbol_is_rejected(self) -> None:
        with INVENTORY_PATH.open(encoding="utf-8") as handle:
            document = json.load(handle)
        document["operations"][0]["owner"]["symbol"] = (
            "DefinitelyMissingOwnershipAnchor"
        )
        errors = CHECKER.validate_inventory(document)
        self.assertTrue(any("symbol has anchor(s) absent" in error for error in errors))

    def test_python_snapshot_debt_cannot_increase_silently(self) -> None:
        with INVENTORY_PATH.open(encoding="utf-8") as handle:
            document = json.load(handle)
        budget = next(
            item
            for item in document["duplicate_debt_budgets"]
            if item["path"] == "web/python/noon.py"
            and item["token"] == "copy.deepcopy"
        )
        budget["maximum"] -= 1
        errors = CHECKER.validate_inventory(document)
        self.assertTrue(any("semantic debt increased" in error for error in errors))

    def test_python_snapshot_debt_budget_must_ratchet_down(self) -> None:
        with INVENTORY_PATH.open(encoding="utf-8") as handle:
            document = json.load(handle)
        budget = next(
            item
            for item in document["duplicate_debt_budgets"]
            if item["path"] == "web/python/noon.py"
            and item["token"] == "copy.deepcopy"
        )
        budget["maximum"] += 1
        errors = CHECKER.validate_inventory(document)
        self.assertTrue(any("budget is stale" in error for error in errors))

    def test_cli_inventory_round_trip(self) -> None:
        # Keep the checker import-level test independent from the process runner while
        # also proving it accepts an equivalent temporary JSON document.
        with INVENTORY_PATH.open(encoding="utf-8") as handle:
            document = json.load(handle)
        with tempfile.NamedTemporaryFile(mode="w", suffix=".json", encoding="utf-8") as handle:
            json.dump(document, handle)
            handle.flush()
            loaded = CHECKER.load_inventory(Path(handle.name))
        self.assertEqual(loaded["schema_version"], 1)


if __name__ == "__main__":
    unittest.main()
