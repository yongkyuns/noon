"""Synthetic admission fixtures only; these tests never execute a GPU."""
from pathlib import Path
import tempfile
import unittest
import contextlib
import io
from types import SimpleNamespace
from unittest.mock import patch

from mixed_cache_evidence import cases, evidence_error, MARKER, PROFILE


class MixedCacheEvidenceTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        (self.root / "PASS.txt").write_text(MARKER + " synthetic checker test")
        (self.root / "profile.txt").write_text("synthetic samples=4\n" + PROFILE)
        for name, size, equal in cases():
            step = int(name.split("-s")[1][0]) if "-s" in name and "-stale" not in name else 3
            if name.endswith("stale-image"):
                step = 4
            if name.endswith("stale-text"):
                step = 5
            expected = self.data(step, size)
            actual = expected if equal else self.data(step - 1 if step >= 4 else 1, size)
            self.write(name, expected, actual)

    @staticmethod
    def data(step, size):
        # Every four-sample pixel has unequal sample bytes, and the full image
        # varies. Steps 0, 2 and 3 intentionally represent the same state.
        code = {0: 10, 1: 20, 2: 10, 3: 10, 4: 30, 5: 40}[step]
        return (bytes((code, 0, 0, 255, 0, code, 0, 255)) * ((size + 7) // 8))[:size]

    def write(self, name, expected, actual):
        (self.root / f"{name}-expected.rgba").write_bytes(expected)
        (self.root / f"{name}-actual.rgba").write_bytes(actual)
        mask = bytes(0 if expected[i:i + 4] == actual[i:i + 4] else 255
                     for i in range(0, len(expected), 4))
        (self.root / f"{name}-diff.mask").write_bytes(mask)

    def test_complete_inventory_and_coherent_fixture(self):
        inventory = list(cases())
        self.assertEqual(len(inventory), 90)
        self.assertEqual(sum(equal for _, _, equal in inventory), 72)
        self.assertIsNone(evidence_error(self.root))

    def test_missing_truncated_or_bad_mask_is_rejected(self):
        path = self.root / "p2-s5-samples-diff.mask"
        path.write_bytes(b"bad")
        self.assertIsNotNone(evidence_error(self.root))
        path.unlink()
        self.assertIsNotNone(evidence_error(self.root))

    def test_ineffective_omission_control_is_rejected(self):
        name = "p0-no-text"
        data = (self.root / f"{name}-expected.rgba").read_bytes()
        self.write(name, data, data)
        self.assertIsNotNone(evidence_error(self.root))

    def test_matching_wrong_pairs_cannot_change_reference(self):
        name = "p1-s1-fresh"
        size = (self.root / f"{name}-actual.rgba").stat().st_size
        self.write(name, self.data(4, size), self.data(4, size))
        self.assertIsNotNone(evidence_error(self.root))

    def test_consistent_history_drift_is_rejected(self):
        for kind in ("ordinary", "resolved", "fresh", "samples"):
            name = f"p0-s2-{kind}"
            size = (self.root / f"{name}-actual.rgba").stat().st_size
            self.write(name, self.data(4, size), self.data(4, size))
        self.assertIsNotNone(evidence_error(self.root))

    def test_stale_control_must_equal_prior_ordinary_frame(self):
        name = "p2-stale-text"
        expected = (self.root / f"{name}-expected.rgba").read_bytes()
        self.write(name, expected, self.data(1, len(expected)))
        self.assertIsNotNone(evidence_error(self.root))

    def test_zero_sample_images_cannot_pass_equal_pairs(self):
        name = "p0-s1-samples"
        size = (self.root / f"{name}-actual.rgba").stat().st_size
        self.write(name, bytes(size), bytes(size))
        self.assertIsNotNone(evidence_error(self.root))

    def stage(self, evidence, *, returncode=0):
        # Mocked Rust execution is admission/control-flow evidence only.
        import qualify_gpu_operator as gate
        log = (f"test {gate.MIXED_TEXT_TEST} ... ok\n{gate.MIXED_TEXT_MARKER}\n"
               "test result: ok. 1 passed; 0 failed; 0 ignored;\n")
        def run(command, **kwargs):
            destination = Path(kwargs["env"]["NOON_CACHE_PROBE_ARTIFACTS"])
            if evidence:
                import shutil
                shutil.copytree(self.root, destination, dirs_exist_ok=True)
            return SimpleNamespace(returncode=returncode, stdout=log)
        with tempfile.TemporaryDirectory() as temp:
            output = Path(temp)
            with patch.object(gate.subprocess, "run", side_effect=run), \
                 contextlib.redirect_stdout(io.StringIO()):
                return gate.run_stage(output, output, "mixed-test", False, test=gate.MIXED_TEXT_TEST)

    def test_runner_requires_mixed_evidence_even_after_success_log(self):
        result = self.stage(False)
        self.assertFalse(result["accepted"])
        self.assertIn("mixed probe evidence", result["evidence_error"])

    def test_runner_accepts_complete_mixed_evidence_and_rejects_corruption(self):
        self.assertTrue(self.stage(True)["accepted"])
        (self.root / "p2-s5-fresh-actual.rgba").write_bytes(b"truncated")
        self.assertFalse(self.stage(True)["accepted"])

    def test_completion_and_profile_are_required(self):
        (self.root / "PASS.txt").unlink()
        self.assertIsNotNone(evidence_error(self.root))
        (self.root / "PASS.txt").write_text(MARKER)
        (self.root / "profile.txt").write_text("samples=4")
        self.assertIsNotNone(evidence_error(self.root))

    def test_runner_rejects_failed_execution_despite_complete_files(self):
        self.assertFalse(self.stage(True, returncode=101)["accepted"])

    def test_own_test_and_completion_marker_are_required(self):
        import qualify_gpu_operator as gate
        success = "test result: ok. 1 passed; 0 failed; 0 ignored;\n"
        log = f"test {gate.MIXED_TEXT_TEST} ... ok\n{gate.MIXED_TEXT_MARKER}\n" + success
        self.assertTrue(gate.qualified(0, log, test=gate.MIXED_TEXT_TEST))
        for broken in (log.replace(gate.MIXED_TEXT_TEST, gate.MIXED_IMAGE_TEST),
                       log.replace(gate.MIXED_TEXT_MARKER, "CACHE_GEOMETRY_PROBE_PASS"),
                       log.replace("1 passed", "0 passed"),
                       log.replace("0 ignored", "1 ignored")):
            self.assertFalse(gate.qualified(0, broken, test=gate.MIXED_TEXT_TEST))

    def test_each_stage_attempt_owns_fresh_artifacts(self):
        import qualify_gpu_operator as gate
        with tempfile.TemporaryDirectory() as temp:
            output = Path(temp)
            fake = SimpleNamespace(returncode=0, stdout="no executed test")
            with patch.object(gate.subprocess, "run", return_value=fake), \
                 contextlib.redirect_stdout(io.StringIO()):
                first = gate.run_stage(output, output, "first", False, test=gate.MIXED_TEXT_TEST)
                (output / first["artifact_directory"] / "PASS.txt").write_text(MARKER)
                second = gate.run_stage(output, output, "second", False, test=gate.MIXED_TEXT_TEST)
        self.assertNotEqual(first["artifact_directory"], second["artifact_directory"])
        self.assertFalse(second["accepted"])


if __name__ == "__main__":
    unittest.main()
