"""Deterministic source-inventory tests; no Manim, WASM, browser or scenes run."""
from __future__ import annotations

import copy
import hashlib
import importlib.util
import json
import shutil
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

SCRIPTS = Path(__file__).resolve().parent
spec = importlib.util.spec_from_file_location("noon_capabilities", SCRIPTS / "noon-capabilities.py")
assert spec and spec.loader
cap = importlib.util.module_from_spec(spec)
spec.loader.exec_module(cap)


class CapabilityTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        for directory in ("scripts", "compat", "web/python/examples"):
            (self.root / directory).mkdir(parents=True)
        for name in ("manim-api-coverage.py", "noon-capabilities.py"):
            shutil.copyfile(SCRIPTS / name, self.root / "scripts" / name)
        (self.root / "web/python/noon.py").write_text(
            '__all__ = ["Circle", "Text", "NewThing"]\nraise RuntimeError("must not import noon")\n',
            encoding="utf-8",
        )
        (self.root / "web/python/_manim_test.py").write_text(
            'public = {"Create": None}\nraise RuntimeError("must not import adapter")\n', encoding="utf-8",
        )
        (self.root / "web/python/examples/circle.py").write_text(
            'raise RuntimeError("must not execute example")\n', encoding="utf-8",
        )
        self.policy = {
            "reference": {"package": "manim", "version": "0.21.0"},
            "statuses": ["supported", "partial", "missing", "blocked", "deferred", "intentional-divergence"],
            "overrides": {
                "Circle": {"status": "supported", "evidence": "fixture test"},
                "Text": {"status": "partial", "reason": "No exact glyph parity", "dependency": "#83"},
                "Axes": {"status": "missing", "dependency": "#85"},
            },
        }
        self.tutorial = {
            "reference": {"version": "0.21.0"},
            "entries": [
                {"id": "circle", "status": "ready", "path": "python/examples/circle.py",
                 "features": ["Circle", "Create"], "upstream": "quickstart", "reuse": "fixture",
                 "parity_status": "candidate", "qualification_mode": "shared-live"},
                {"id": "plot", "status": "blocked", "dependency": "#85", "features": ["Axes"]},
            ],
        }
        self.save()

    def save(self):
        (self.root / cap.POLICY).write_text(json.dumps(self.policy), encoding="utf-8")
        (self.root / cap.TUTORIALS).write_text(json.dumps(self.tutorial), encoding="utf-8")

    def report(self):
        self.save()
        return cap.build_report(self.root)

    def test_source_inventory_is_not_runtime_qualification(self):
        report = self.report()
        self.assertEqual(report["schema_version"], 1)
        self.assertEqual(report["scope"], "source-inventory")
        self.assertFalse(report["qualification"]["behavioral_tests_run"])
        self.assertTrue(all(value is None for value in report["runtime"].values()))
        self.assertIsNone(report["provenance"]["revision"])
        self.assertFalse(report["symbols"]["Circle"]["runtime_verified"])

    def test_noon_adapter_and_example_are_not_executed(self):
        report = self.report()  # All three fixture files would raise on execution.
        self.assertTrue(report["symbols"]["Create"]["exported"])
        self.assertNotIn("manim", sys.modules)

    def test_unclassified_export_cannot_be_promoted(self):
        row = self.report()["symbols"]["NewThing"]
        self.assertEqual(row["policy"]["status"], "partial")
        self.assertEqual(row["classification_source"], "unclassified-export")

    def test_restrictions_and_dependencies_survive(self):
        row = self.report()["symbols"]["Text"]
        self.assertEqual(row["policy"], self.policy["overrides"]["Text"])
        self.assertEqual(row["ready_examples"], [])

    def test_ready_candidate_is_not_parity_qualified(self):
        row = self.report()["examples"]["circle"]
        self.assertEqual(row["parity_status"], "candidate")
        self.assertEqual(row["qualification_mode"], "shared-live")
        self.assertFalse(row["runtime_verified"])

    def test_qualified_label_needs_fixture(self):
        self.tutorial["entries"][0]["parity_status"] = "parity-qualified"
        with self.assertRaisesRegex(ValueError, "requires a parity fixture"):
            self.report()

    def test_unknown_parity_label_fails(self):
        self.tutorial["entries"][0]["parity_status"] = "looks-good"
        with self.assertRaisesRegex(ValueError, "invalid parity_status"):
            self.report()

    def test_supported_requires_export(self):
        self.policy["overrides"]["Ghost"] = {"status": "supported", "evidence": "test"}
        with self.assertRaisesRegex(ValueError, "export is absent"):
            self.report()

    def test_supported_requires_evidence(self):
        del self.policy["overrides"]["Circle"]["evidence"]
        with self.assertRaisesRegex(ValueError, "requires declared evidence"):
            self.report()

    def test_blocked_export_with_ready_evidence_fails(self):
        self.policy["overrides"]["Circle"]["status"] = "blocked"
        with self.assertRaisesRegex(ValueError, "blocked export"):
            self.report()

    def test_missing_inventory_fails(self):
        (self.root / cap.TUTORIALS).unlink()
        with self.assertRaisesRegex(ValueError, "missing or unconfined"):
            cap.build_report(self.root)

    def test_malformed_inventory_fails(self):
        (self.root / cap.POLICY).write_text("[]", encoding="utf-8")
        with self.assertRaisesRegex(ValueError, "JSON object"):
            cap.build_report(self.root)

    def test_version_mismatch_fails(self):
        self.tutorial["reference"]["version"] = "99"
        with self.assertRaisesRegex(ValueError, "versions differ"):
            self.report()

    def test_invalid_policy_status_fails(self):
        self.policy["overrides"]["Circle"]["status"] = "probably"
        with self.assertRaisesRegex(ValueError, "invalid policy status"):
            self.report()

    def test_duplicate_example_fails(self):
        self.tutorial["entries"].append(copy.deepcopy(self.tutorial["entries"][0]))
        with self.assertRaisesRegex(ValueError, "duplicate id"):
            self.report()

    def test_missing_ready_example_fails(self):
        (self.root / "web/python/examples/circle.py").unlink()
        with self.assertRaisesRegex(ValueError, "missing fixture"):
            self.report()

    def test_parent_traversal_is_rejected_even_when_file_exists(self):
        self.tutorial["entries"][0]["path"] = "../web/python/examples/circle.py"
        with self.assertRaisesRegex(ValueError, "unsafe repository path"):
            self.report()

    def test_absolute_example_is_rejected(self):
        self.tutorial["entries"][0]["path"] = str(self.root / "web/python/examples/circle.py")
        with self.assertRaisesRegex(ValueError, "unsafe absolute"):
            self.report()

    def test_symlink_escape_is_rejected(self):
        with tempfile.TemporaryDirectory() as outside:
            target = Path(outside) / "outside.py"
            target.write_text("secret", encoding="utf-8")
            fixture = self.root / "web/python/examples/circle.py"
            fixture.unlink()
            fixture.symlink_to(target)
            with self.assertRaisesRegex(ValueError, "unconfined"):
                self.report()

    def test_invalid_features_fail(self):
        self.tutorial["entries"][0]["features"] = "Circle"
        with self.assertRaisesRegex(ValueError, "features must be"):
            self.report()

    def test_hashes_cover_inputs_and_change_with_source(self):
        before = self.report()
        path = "web/python/examples/circle.py"
        expected = hashlib.sha256((self.root / path).read_bytes()).hexdigest()
        self.assertEqual(before["provenance"]["input_sha256"][path], expected)
        self.assertEqual(before["examples"]["circle"]["source_sha256"], expected)
        (self.root / path).write_text("# changed\n", encoding="utf-8")
        self.assertNotEqual(self.report()["examples"]["circle"]["source_sha256"], expected)

    def test_output_is_deterministic(self):
        self.assertEqual(self.report(), self.report())

    def test_symbol_filter_returns_relevant_ready_examples(self):
        report = self.report()
        selected = cap.select_report(report, ["Circle"], [])
        self.assertEqual(list(selected["symbols"]), ["Circle"])
        self.assertEqual(list(selected["examples"]), ["circle"])
        self.assertIn("Text", report["symbols"])  # No mutation of input.

    def test_unknown_symbol_and_example_fail(self):
        report = self.report()
        for symbols, examples in ((["typo"], []), ([], ["typo"])):
            with self.subTest(symbols=symbols, examples=examples):
                with self.assertRaisesRegex(ValueError, "unknown"):
                    cap.select_report(report, symbols, examples)

    def test_blocked_example_is_visible_but_not_ready_evidence(self):
        selected = cap.select_report(self.report(), ["Axes"], ["plot"])
        self.assertEqual(selected["examples"]["plot"]["status"], "blocked")
        self.assertEqual(selected["symbols"]["Axes"]["ready_examples"], [])

    def test_cli_works_from_another_directory_without_site_packages(self):
        result = subprocess.run(
            [sys.executable, "-S", "-B", str(self.root / "scripts/noon-capabilities.py"), "--symbol", "Circle"],
            cwd="/", capture_output=True, text=True, timeout=10,
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(list(json.loads(result.stdout)["symbols"]), ["Circle"])
        self.assertEqual(result.stderr, "")

    def test_cli_invalid_query_has_no_success_output(self):
        result = subprocess.run(
            [sys.executable, "-S", "-B", str(self.root / "scripts/noon-capabilities.py"), "--symbol", "typo"],
            capture_output=True, text=True, timeout=10,
        )
        self.assertEqual(result.returncode, 1)
        self.assertEqual(result.stdout, "")
        self.assertFalse(json.loads(result.stderr)["ok"])

    def test_git_unavailable_is_not_a_fake_revision(self):
        with patch.object(cap.subprocess, "check_output", side_effect=FileNotFoundError):
            self.assertEqual(cap.source_revision(self.root), {"revision": None, "dirty": None})


class SkillTests(unittest.TestCase):
    save = CapabilityTests.save
    report = CapabilityTests.report
    def setUp(self):
        CapabilityTests.setUp(self)
        spec = importlib.util.spec_from_file_location("skill_check", SCRIPTS / "check-noon-agent-skill.py")
        assert spec and spec.loader
        self.checker = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(self.checker)
        self.skill = self.root / "skills/noon-authoring"
        (self.skill / "references").mkdir(parents=True)
        (self.skill / "SKILL.md").write_text(
            "---\nname: noon-authoring\ndescription: Author Noon scenes.\ncompatibility: Trusted Noon checkout.\n---\n"
            "[Examples](references/examples.md)\n", encoding="utf-8",
        )
        (self.skill / "references/examples.md").write_text("| `circle` | Basic geometry. |\n", encoding="utf-8")
        for name in self.checker.COMMANDS:
            path = self.root / name
            if not path.exists():
                path.write_text("# fixture entrypoint\n", encoding="utf-8")

    def test_skill_valid_references(self):
        self.assertEqual(self.checker.validate_skill(self.root, self.report()), 1)

    def test_skill_unready_reference_fails(self):
        (self.skill / "references/examples.md").write_text("| `plot` | Plot. |\n", encoding="utf-8")
        with self.assertRaisesRegex(ValueError, "not ready"):
            self.checker.validate_skill(self.root, self.report())

    def test_skill_dead_link_fails(self):
        with (self.skill / "SKILL.md").open("a", encoding="utf-8") as out:
            out.write("[Missing](references/missing.md)\n")
        with self.assertRaisesRegex(ValueError, "missing or unconfined link"):
            self.checker.validate_skill(self.root, self.report())

    def test_skill_wrong_name_fails(self):
        path = self.skill / "SKILL.md"
        path.write_text(path.read_text().replace("name: noon-authoring", "name: manim"), encoding="utf-8")
        with self.assertRaisesRegex(ValueError, "skill name"):
            self.checker.validate_skill(self.root, self.report())

    def test_skill_missing_command_fails(self):
        (self.root / "scripts/check.sh").unlink()
        with self.assertRaisesRegex(ValueError, "missing documented command"):
            self.checker.validate_skill(self.root, self.report())


if __name__ == "__main__":
    unittest.main()
