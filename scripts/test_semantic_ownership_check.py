#!/usr/bin/env python3
"""Focused regressions for the semantic ownership inventory validator."""

from __future__ import annotations

import copy
import unittest

import semantic_ownership_check as ownership


class SemanticOwnershipCheckTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.inventory = ownership.load_inventory()

    def duplicate_operation(self) -> dict:
        # The production inventory need not retain debt to test validator failures.
        operation = next(
            copy.deepcopy(operation)
            for operation in self.inventory["operations"]
            if operation["classification"] == "shared-rust"
            and any(adapter["language"] == "python" for adapter in operation.get("adapters", []))
        )
        operation["shared_owner"] = operation["owner"]
        operation["owner"] = next(
            adapter for adapter in operation.pop("adapters") if adapter["language"] == "python"
        )
        operation["classification"] = "python-semantic-duplicate"
        operation["migration_issue"] = "#61"
        operation["replacement"] = "Validator fixture: replace hypothetical Python semantics with the shared owner."
        return operation

    def inventory_with_only(self, operation: dict) -> dict:
        document = copy.deepcopy(self.inventory)
        document["operations"] = [operation]
        return document

    def test_checked_in_inventory_is_valid(self) -> None:
        self.assertEqual(ownership.validate_inventory(copy.deepcopy(self.inventory)), [])

    def test_synthetic_duplicate_fixture_is_valid(self) -> None:
        self.assertEqual(ownership.validate_inventory(self.inventory_with_only(self.duplicate_operation())), [])

    def test_python_semantic_duplicate_must_be_python_owned(self) -> None:
        operation = self.duplicate_operation()
        operation["owner"]["language"] = "rust"

        errors = ownership.validate_inventory(self.inventory_with_only(operation))

        self.assertTrue(
            any("python-semantic-duplicate entries must be Python-owned" in error for error in errors),
            errors,
        )

    def test_python_semantic_duplicate_shared_owner_must_be_rust_owned(self) -> None:
        operation = self.duplicate_operation()
        operation["shared_owner"]["language"] = "python"

        errors = ownership.validate_inventory(self.inventory_with_only(operation))

        self.assertTrue(
            any(
                "python-semantic-duplicate shared_owner must be Rust-owned" in error
                for error in errors
            ),
            errors,
        )


if __name__ == "__main__":
    unittest.main()
