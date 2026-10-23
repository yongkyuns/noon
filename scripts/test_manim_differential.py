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
from unittest.mock import Mock, patch


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

    @unittest.skipIf(differential.manim is None, "pinned CE reference is not installed")
    def test_point_append_matrix_matches_every_original_case_and_hook(self):
        fixture = json.loads((SOURCE.parent.parent / "fixtures/manim-v0.21/point-append.json").read_text())
        profile = differential._point_append_argument_profiles(differential.manim)
        self.assertEqual(profile["values"]["VMobject"], fixture["cases"])
        self.assertEqual(profile["values"]["VGroup"], fixture["familyCases"])
        self.assertEqual(profile["hooks"], fixture["hookCases"])
        for batch in profile["ordinary_batch"].values():
            self.assertEqual(batch["output"], {
                "kind": "raised", "type": "ValueError",
                "message": "could not broadcast input array from shape (24,) into shape (24,3)",
            })
            self.assertEqual(batch["pointsXYZ"], [])
            self.assertEqual(batch["numPoints"], 0)
            self.assertTrue(batch["inputUnchanged"])

    def test_complete_matches_succeed_without_reference_errors(self):
        fixture = differential.Fixture("matching", lambda: None, lambda: [1.0, 2.0])
        code, report = self.run_comparison([fixture], {"matching": [1.0, 2.0]})
        self.assertEqual(code, 0)
        self.assertEqual(report["fixtures"][0]["status"], "pass")
        self.assertNotIn("reference_error", report["fixtures"][0])

    def test_circle_probe_retains_errors_and_later_successes(self):
        receiver = Mock()
        receiver.shift.return_value = receiver
        receiver.rotate.return_value = receiver
        receiver.get_fill_opacity.return_value = 0
        receiver.get_stroke_opacity.return_value = 1
        receiver.get_stroke_width.return_value = 4

        def surround(target, **options):
            if options.get("dim_to_match") == -3:
                raise AssertionError()
            return receiver

        receiver.surround.side_effect = surround
        provider = SimpleNamespace(
            Circle=Mock(return_value=receiver), Rectangle=Mock(return_value=receiver),
            Ellipse=Mock(return_value=receiver), Dot=Mock(return_value=receiver),
            Line=Mock(return_value=receiver), VGroup=Mock(return_value=receiver),
            RIGHT=1, LEFT=-1, UP=1, DOWN=-1,
        )
        observation = {"center": [0, 0], "width": 2, "height": 2}
        with patch.object(differential, "_object_observation", return_value=observation):
            result = differential._circle_surround_probe(provider)
        self.assertEqual(len(result), 26)
        self.assertEqual(result["negative_width"]["error_type"], "AssertionError")
        self.assertFalse(result["negative_width"]["returns_self"])
        self.assertIsNone(result["shared_depth"]["error_type"])
        self.assertTrue(result["shared_depth"]["returns_self"])


if __name__ == "__main__":
    unittest.main()
