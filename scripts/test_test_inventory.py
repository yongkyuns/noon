#!/usr/bin/env python3
"""Ensure the inventory recognizes real host tests without hiding missing tests."""

import importlib.util
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch


spec = importlib.util.spec_from_file_location(
    "test_inventory", Path(__file__).with_name("test-inventory.py")
)
inventory = importlib.util.module_from_spec(spec)
spec.loader.exec_module(inventory)


class NativeHostInventoryTests(unittest.TestCase):
    def test_real_cpython_entry_points_count_as_host_integration(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source = root / "crates/noon-python/src/lib.rs"
            source.parent.mkdir(parents=True)
            source.write_text("// Native extension exercised by CPython.\n")
            test = root / "scripts/test_native_python.py"
            test.parent.mkdir()
            test.write_text("# Real extension test entry point.\n")
            with patch.object(inventory, "ROOT", root):
                report = inventory.collect()
            module = report["production_modules"][0]
            self.assertEqual(module["strategies"], ["native-host-integration"])
            self.assertEqual(module["host_test_entry_points"], ["scripts/test_native_python.py"])
            self.assertEqual(report["summary"]["uncovered_modules"], [])
            self.assertEqual(report["tests"][0]["layer"], "noon-python")
            self.assertEqual(report["tests"][0]["type"], "native-host-integration")

    def test_absent_host_entry_points_do_not_silently_qualify_crate(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source = root / "crates/noon-python/src/lib.rs"
            source.parent.mkdir(parents=True)
            source.write_text("// No test entry point exists.\n")
            with patch.object(inventory, "ROOT", root):
                report = inventory.collect()
            self.assertEqual(
                report["summary"]["uncovered_modules"],
                ["crates/noon-python/src/lib.rs"],
            )
            self.assertEqual(report["tests"], [])


if __name__ == "__main__":
    unittest.main()
