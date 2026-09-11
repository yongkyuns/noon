#!/usr/bin/env python3
from __future__ import annotations

import importlib.util
import json
import os
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
        (self.root / "compat").mkdir()
        (self.root / "scripts").mkdir()
        (self.root / "web/python/examples").mkdir(parents=True)
        (self.root / "web/python/_manim_compat.py").write_text("EXPORTED = ('Circle', 'Transform', 'Axes')\n", encoding="utf-8")
        (self.root / "web/python/noon.py").write_text("from _manim_compat import *\n", encoding="utf-8")
        (self.root / "scripts/manim-api-coverage.py").write_text(
            """from pathlib import Path\n"
            "def validate_tutorial_examples(entries):\n"
            "    ids=[e.get('id') for e in entries]\n"
            "    return ['duplicate tutorial example id'] if len(ids)!=len(set(ids)) else []\n"
            "def noon_public_exports(): return {'Circle','Transform','Axes'}\n"
            "def browser_evidence_for(name, entries):\n"
            "    return [e['id'] for e in entries if e.get('status')=='ready' and name in e.get('features',[])]\n"
            """, encoding="utf-8")
        shutil.copy2(SCRIPTS / "noon-capabilities.py", self.root / "scripts/noon-capabilities.py")
        self.policy = {
            "reference": {"package": "manim", "version": "0.21.0"},
            "statuses": ["supported", "partial", "blocked"],
            "overrides": {
                "Circle": {"status": "supported", "evidence": "circle browser fixture"},
                "Transform": {"status": "partial", "reason": "subset"},
                "Axes": {"status": "blocked", "reason": "plotting unavailable"},
            },
        }
        self.tutorial = {
            "reference": {"version": "0.21.0"},
            "entries": [
                {"id": "circle", "status": "ready", "path": "python/examples/circle.py", "features": ["Circle"]},
                {"id": "transform", "status": "ready", "path": "python/examples/transform.py", "features": ["Transform"], "parity_status": "candidate"},
                {"id": "plot", "status": "blocked", "path": "python/examples/plot.py", "features": ["Axes"]},
            ],
        }
        self.save()
        (self.root / "web/python/examples/circle.py").write_text("# circle\n", encoding="utf-8")
        (self.root / "web/python/examples/transform.py").write_text("# transform\n", encoding="utf-8")
        (self.root / "web/python/examples/plot.py").write_text("# plot\n", encoding="utf-8")

    def save(self):
        (self.root / "compat/manim-v0.21.0.json").write_text(json.dumps(self.policy), encoding="utf-8")
        (self.root / "web/python/examples/manim_tutorial_manifest.json").write_text(json.dumps(self.tutorial), encoding="utf-8")

    def report(self):
        return cap.build_report(self.root)

    def test_source_inventory_is_not_runtime_qualification(self):
        report = self.report()
        self.assertEqual(report["scope"], "source-inventory")
        self.assertFalse(report["qualification"]["behavioral_tests_run"])
        self.assertFalse(report["symbols"]["Circle"]["runtime_verified"])
        self.assertFalse(report["examples"]["circle"]["runtime_verified"])

    def test_hashes_cover_inputs_and_change_with_source(self):
        first = self.report()
        hashes = first["provenance"]["input_sha256"]
        self.assertIn("web/python/examples/circle.py", hashes)
        self.assertIn("scripts/noon-capabilities.py", hashes)
        self.assertRegex(hashes["web/python/examples/circle.py"], r"^[0-9a-f]{64}$")
        self.assertEqual(first["examples"]["circle"]["source_sha256"], hashes["web/python/examples/circle.py"])
        (self.root / "web/python/examples/circle.py").write_text("# changed\n", encoding="utf-8")
        second = self.report()
        self.assertNotEqual(first["examples"]["circle"]["source_sha256"], second["examples"]["circle"]["source_sha256"])

    def test_output_is_deterministic(self):
        self.assertEqual(self.report(), self.report())

    def test_symbol_filter_returns_relevant_ready_examples(self):
        selected = cap.select_report(self.report(), ["Circle"], [])
        self.assertEqual(set(selected["symbols"]), {"Circle"})
        self.assertEqual(set(selected["examples"]), {"circle"})

    def test_unknown_symbol_and_example_fail(self):
        report = self.report()
        with self.assertRaisesRegex(ValueError, "unknown symbols"):
            cap.select_report(report, ["Nope"], [])
        with self.assertRaisesRegex(ValueError, "unknown examples"):
            cap.select_report(report, [], ["nope"])

    def test_supported_requires_export(self):
        self.policy["overrides"]["Missing"] = {"status": "supported", "evidence": "none"}
        self.save()
        with self.assertRaisesRegex(ValueError, "export is absent"):
            self.report()

    def test_supported_requires_evidence(self):
        del self.policy["overrides"]["Circle"]["evidence"]
        self.save()
        with self.assertRaisesRegex(ValueError, "requires declared evidence"):
            self.report()

    def test_blocked_export_with_ready_evidence_fails(self):
        self.policy["overrides"]["Circle"] = {"status": "blocked", "reason": "bad"}
        self.save()
        with self.assertRaisesRegex(ValueError, "blocked export has ready tutorial evidence"):
            self.report()

    def test_unclassified_export_cannot_be_promoted(self):
        del self.policy["overrides"]["Transform"]
        self.save()
        row = self.report()["symbols"]["Transform"]
        self.assertEqual(row["classification_source"], "unclassified-export")
        self.assertEqual(row["policy"]["status"], "partial")
        self.assertIn("Statically exported", row["policy"]["reason"])

    def test_restrictions_and_dependencies_survive(self):
        self.policy["overrides"]["Transform"]["restriction"] = "same-family only"
        self.policy["overrides"]["Transform"]["dependency"] = "supported mobject"
        self.save()
        row = self.report()["symbols"]["Transform"]["policy"]
        self.assertEqual(row["restriction"], "same-family only")
        self.assertEqual(row["dependency"], "supported mobject")

    def test_ready_candidate_is_not_parity_qualified(self):
        row = self.report()["examples"]["transform"]
        self.assertEqual(row["status"], "ready")
        self.assertEqual(row["parity_status"], "candidate")
        self.assertNotIn("parity_fixture", row)

    def test_qualified_label_needs_fixture(self):
        self.tutorial["entries"][0]["parity_status"] = "parity-qualified"
        self.save()
        with self.assertRaisesRegex(ValueError, "requires a parity fixture"):
            self.report()

    def test_blocked_example_is_visible_but_not_ready_evidence(self):
        report = self.report()
        self.assertIn("plot", report["examples"])
        self.assertEqual(report["examples"]["plot"]["status"], "blocked")
        self.assertNotIn("source_sha256", report["examples"]["plot"])
        self.assertEqual(report["symbols"]["Axes"]["ready_examples"], [])

    def test_missing_inventory_fails(self):
        (self.root / "compat/manim-v0.21.0.json").unlink()
        with self.assertRaises(OSError):
            self.report()

    def test_malformed_inventory_fails(self):
        (self.root / "compat/manim-v0.21.0.json").write_text("[]", encoding="utf-8")
        with self.assertRaisesRegex(ValueError, "expected a JSON object"):
            self.report()

    def test_version_mismatch_fails(self):
        self.tutorial["reference"]["version"] = "0.20.0"
        self.save()
        with self.assertRaisesRegex(ValueError, "versions differ"):
            self.report()

    def test_invalid_policy_status_fails(self):
        self.policy["overrides"]["Circle"]["status"] = "working"
        self.save()
        with self.assertRaisesRegex(ValueError, "invalid policy status"):
            self.report()

    def test_invalid_features_fail(self):
        self.tutorial["entries"][0]["features"] = "Circle"
        self.save()
        with self.assertRaisesRegex(ValueError, "features must be a string array"):
            self.report()

    def test_unknown_parity_label_fails(self):
        self.tutorial["entries"][0]["parity_status"] = "gold"
        self.save()
        with self.assertRaisesRegex(ValueError, "invalid parity_status"):
            self.report()

    def test_duplicate_example_fails(self):
        self.tutorial["entries"].append(dict(self.tutorial["entries"][0]))
        self.save()
        with self.assertRaisesRegex(ValueError, "duplicate tutorial example id"):
            self.report()

    def test_missing_ready_example_fails(self):
        (self.root / "web/python/examples/circle.py").unlink()
        with self.assertRaisesRegex(ValueError, "missing or unconfined repository file"):
            self.report()

    def test_absolute_example_is_rejected(self):
        self.tutorial["entries"][0]["path"] = "/tmp/scene.py"
        self.save()
        with self.assertRaisesRegex(ValueError, "unsafe absolute example path"):
            self.report()

    def test_parent_traversal_is_rejected_even_when_file_exists(self):
        (self.root / "web/escape.py").write_text("# escape\n", encoding="utf-8")
        self.tutorial["entries"][0]["path"] = "python/examples/../../escape.py"
        self.save()
        with self.assertRaisesRegex(ValueError, "unsafe repository path"):
            self.report()

    def test_symlink_escape_is_rejected(self):
        outside = Path(self.temp.name).parent / "outside-noon-capability.py"
        outside.write_text("# outside\n", encoding="utf-8")
        self.addCleanup(lambda: outside.unlink(missing_ok=True))
        target = self.root / "web/python/examples/circle.py"
        target.unlink()
        target.symlink_to(outside)
        with self.assertRaisesRegex(ValueError, "missing or unconfined repository file"):
            self.report()

    def test_noon_adapter_and_example_are_not_executed(self):
        (self.root / "web/python/noon.py").write_text("raise RuntimeError('must not execute noon.py')\n", encoding="utf-8")
        (self.root / "web/python/examples/circle.py").write_text("raise RuntimeError('must not execute scene')\n", encoding="utf-8")
        report = self.report()
        self.assertIn("Circle", report["symbols"])
        self.assertRegex(report["examples"]["circle"]["source_sha256"], r"^[0-9a-f]{64}$")

    def test_static_module_exports_preserve_literal_and_unpacked_names(self):
        (self.root / "web/python/_manim_more.py").write_text("EXPORTED = ('Line',)\n", encoding="utf-8")
        (self.root / "web/python/_manim_compat.py").write_text(
            "EXPORTED = ('Circle', 'Transform', 'Axes')\nfrom _manim_more import *\n", encoding="utf-8")
        self.policy["overrides"]["Line"] = {"status": "partial", "reason": "line subset"}
        self.save()
        report = self.report()
        self.assertIn("Line", report["symbols"])
        self.assertTrue(report["symbols"]["Line"]["exported"])

    def test_cli_works_from_another_directory_without_site_packages(self):
        result = subprocess.run(
            [sys.executable, "-S", "-B", str(self.root / "scripts/noon-capabilities.py"), "--symbol", "Circle"],
            cwd=Path(self.temp.name).parent, capture_output=True, text=True, timeout=10,
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        report = json.loads(result.stdout)
        self.assertEqual(set(report["symbols"]), {"Circle"})

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
                path.parent.mkdir(parents=True, exist_ok=True)
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
