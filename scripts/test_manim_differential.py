"""Keep a failed reference probe from discarding the other parity evidence."""

import contextlib
import importlib.util
import io
import json
from pathlib import Path
import sys
import tempfile
from types import SimpleNamespace
import unittest
from unittest.mock import patch


SOURCE = Path(__file__).with_name("manim-differential.py")
SPEC = importlib.util.spec_from_file_location("manim_differential", SOURCE)
differential = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = differential
SPEC.loader.exec_module(differential)


class DifferentialReportTests(unittest.TestCase):
    def run_comparison(self, fixtures, observations):
        reference = SimpleNamespace(
            __version__=differential.PINNED_MANIM_VERSION,
            logger=SimpleNamespace(handlers=[]),
        )
        output = io.StringIO()
        with tempfile.TemporaryDirectory(prefix="noon-differential-report-") as directory:
            source = Path(directory, "noon.json")
            source.write_text(json.dumps(observations))
            with (
                patch.object(differential, "FIXTURES", fixtures),
                patch.object(differential, "manim", reference),
                patch.object(sys, "argv", [str(SOURCE), "--json", "--noon-observations", str(source)]),
                contextlib.redirect_stdout(output),
            ):
                result = differential.main()
        return result, json.loads(output.getvalue())

    def test_reference_error_preserves_later_matches_and_mismatches_and_fails(self):
        def invalid_reference():
            raise ValueError("three coordinates required")

        fixture = differential.Fixture
        fixtures = [
            fixture("invalid", lambda: None, invalid_reference),
            fixture("matching", lambda: None, lambda: [2.0, 3.0]),
            fixture("mismatch", lambda: None, lambda: 5.0),
        ]
        code, report = self.run_comparison(
            fixtures, {"invalid": [1, 2], "matching": [2.0, 3.0], "mismatch": 4.0}
        )
        self.assertEqual(code, 1)
        invalid, matching, mismatch = report["fixtures"]
        self.assertEqual(invalid["status"], "error")
        self.assertEqual(invalid["reference_error"], {
            "type": "ValueError", "message": "three coordinates required",
        })
        self.assertEqual(invalid["noon"], [1, 2])
        self.assertIn("three coordinates required", invalid["differences"][0])
        self.assertEqual(matching["status"], "pass")
        self.assertEqual(mismatch["status"], "mismatch")
        self.assertTrue(mismatch["differences"])

    def test_complete_matches_succeed_without_reference_errors(self):
        fixture = differential.Fixture("matching", lambda: None, lambda: [1.0, 2.0])
        code, report = self.run_comparison([fixture], {"matching": [1.0, 2.0]})
        self.assertEqual(code, 0)
        self.assertEqual(report["fixtures"][0]["status"], "pass")
        self.assertNotIn("reference_error", report["fixtures"][0])


if __name__ == "__main__":
    unittest.main()
