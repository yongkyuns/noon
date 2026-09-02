import importlib.util
import json
import types
import unittest
from pathlib import Path


ROOT = Path(__file__).parents[2]
SCRIPT = ROOT / "scripts" / "manim-api-coverage.py"
SPEC = importlib.util.spec_from_file_location("manim_api_coverage", SCRIPT)
assert SPEC is not None and SPEC.loader is not None
coverage = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(coverage)


class ManimApiCoveragePolicyTests(unittest.TestCase):
    def test_exported_facade_with_ready_browser_evidence_cannot_remain_blocked(self) -> None:
        policy = {
            "reference": {"version": "0.21.0"},
            "statuses": ["supported", "partial", "blocked", "deferred", "missing"],
            "overrides": {},
            "default": {"status": "missing", "category": "other"},
        }
        rows = {
            "Text": {
                "status": "blocked",
                "category": "text-math",
                "noon_exported": True,
            },
            "Axes": {
                "status": "blocked",
                "category": "plotting",
                "noon_exported": False,
            },
            "ThreeDScene": {
                "status": "deferred",
                "category": "3d",
                "noon_exported": False,
            },
        }
        tutorial_examples = [
            {"id": "text-fixture", "status": "ready", "features": ["Text"]},
            {"id": "plot-backlog", "status": "blocked", "features": ["Axes"]},
        ]

        errors = coverage.validate(
            policy,
            types.SimpleNamespace(__version__="0.21.0"),
            rows,
            tutorial_examples,
        )

        self.assertTrue(any("Text" in error and "ready browser evidence" in error for error in errors))
        self.assertFalse(any("Axes" in error and "ready browser evidence" in error for error in errors))
        self.assertFalse(any("ThreeDScene" in error and "ready browser evidence" in error for error in errors))

    def test_current_policy_records_exported_text_and_unexported_plotting(self) -> None:
        policy = json.loads(
            (ROOT / "compat" / "manim-v0.21.0.json").read_text(encoding="utf-8")
        )
        self.assertEqual(policy["overrides"]["Text"]["status"], "partial")
        self.assertEqual(policy["overrides"]["Typst"]["status"], "partial")
        for name in (
            "Axes",
            "NumberLine",
            "NumberPlane",
            "ComplexPlane",
            "PolarPlane",
            "ParametricFunction",
            "FunctionGraph",
            "ImplicitFunction",
        ):
            with self.subTest(name=name):
                self.assertEqual(policy["overrides"][name]["status"], "missing")

    def test_static_export_audit_captures_dynamic_public_mapping(self) -> None:
        self.assertIn("Write", coverage.noon_public_exports())
        self.assertIn("Unwrite", coverage.noon_public_exports())


if __name__ == "__main__":
    unittest.main()
