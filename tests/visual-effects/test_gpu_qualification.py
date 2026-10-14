"""Control-flow/parser checks only; these are not GPU execution evidence."""
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch
from types import SimpleNamespace

from qualify_gpu_operator import cache_evidence_cases, cache_evidence_error
from qualify_gpu_operator import CACHE_TARGET, CACHE_TEST, run_stage, HOST_TEST, ANIMATED_TEST, LIVE_TEST, PAINTER_TEST, SEMANTIC_TEST, TEST, qualified


class QualificationAdmission(unittest.TestCase):
    def test_exact_success_required(self):
        log = f"test {TEST} ... ok\ntest result: ok. 1 passed; 0 failed; 0 ignored;"
        self.assertTrue(qualified(0, log))
        self.assertFalse(qualified(1, log))
        self.assertFalse(qualified(0, log.replace(TEST, "unrelated_test")))
        self.assertFalse(qualified(0, log.replace("1 passed", "0 passed")))
        self.assertFalse(qualified(0, log.replace("0 ignored", "1 ignored")))

    def test_only_measured_mask_failure_qualifies_mutation(self):
        log = (
            f"test {TEST} ... FAILED\n"
            "Gaussian mask exceeds frozen tolerance: 0.002\n"
            "test result: FAILED. 0 passed; 1 failed; 0 ignored;"
        )
        self.assertTrue(qualified(101, log, negative=True))
        self.assertFalse(qualified(0, log, negative=True))
        self.assertFalse(qualified(-1, log, negative=True))
        self.assertFalse(qualified(101, log.replace(TEST, "other"), negative=True))
        for failure in ("no adapter", "compile error E0603", "assertion failed"):
            self.assertFalse(qualified(101, log.replace(
                "Gaussian mask exceeds frozen tolerance: 0.002", failure
            ), negative=True))

    def test_painter_stage_cannot_be_satisfied_by_the_operator_test(self):
        operator = f"test {TEST} ... ok\ntest result: ok. 1 passed; 0 failed; 0 ignored;"
        painter = operator.replace(TEST, PAINTER_TEST)
        self.assertFalse(qualified(0, operator, test=PAINTER_TEST))
        self.assertFalse(qualified(0, painter))
        self.assertTrue(qualified(0, painter, test=PAINTER_TEST))
        self.assertFalse(qualified(0, painter.replace("1 passed", "0 passed"), test=PAINTER_TEST))
        self.assertFalse(qualified(101, painter, test=PAINTER_TEST))

    def test_semantic_stage_requires_its_own_executed_test(self):
        for other in (TEST, PAINTER_TEST):
            log = f"test {other} ... ok\ntest result: ok. 1 passed; 0 failed; 0 ignored;"
            self.assertFalse(qualified(0, log, test=SEMANTIC_TEST))
        log = f"test {SEMANTIC_TEST} ... ok\ntest result: ok. 1 passed; 0 failed; 0 ignored;"
        self.assertTrue(qualified(0, log, test=SEMANTIC_TEST))
        self.assertFalse(qualified(0, log.replace("1 passed", "0 passed"), test=SEMANTIC_TEST))

    def test_animated_stage_cannot_reuse_static_or_operator_success(self):
        for other in (TEST, PAINTER_TEST, SEMANTIC_TEST):
            log = f"test {other} ... ok\ntest result: ok. 1 passed; 0 failed; 0 ignored;"
            self.assertFalse(qualified(0, log, test=ANIMATED_TEST))
        log = f"test {ANIMATED_TEST} ... ok\ntest result: ok. 1 passed; 0 failed; 0 ignored;"
        self.assertTrue(qualified(0, log, test=ANIMATED_TEST))
        self.assertFalse(qualified(101, log, test=ANIMATED_TEST))
        self.assertFalse(qualified(0, log.replace("1 passed", "0 passed"), test=ANIMATED_TEST))

    def test_live_stage_requires_an_executed_live_test(self):
        for other in (TEST, PAINTER_TEST, SEMANTIC_TEST, ANIMATED_TEST):
            log = f"test {other} ... ok\\ntest result: ok. 1 passed; 0 failed; 0 ignored;"
            self.assertFalse(qualified(0, log, test=LIVE_TEST))
        log = f"test {LIVE_TEST} ... ok\\ntest result: ok. 1 passed; 0 failed; 0 ignored;"
        self.assertTrue(qualified(0, log, test=LIVE_TEST))
        self.assertFalse(qualified(101, log, test=LIVE_TEST))
        self.assertFalse(qualified(0, log.replace("1 passed", "0 passed"), test=LIVE_TEST))

    def test_host_stage_requires_an_executed_host_test(self):
        for other in (TEST, PAINTER_TEST, SEMANTIC_TEST, ANIMATED_TEST, LIVE_TEST):
            log = f"test {other} ... ok\ntest result: ok. 1 passed; 0 failed; 0 ignored;"
            self.assertFalse(qualified(0, log, test=HOST_TEST))
        log = f"test {HOST_TEST} ... ok\ntest result: ok. 1 passed; 0 failed; 0 ignored;"
        self.assertTrue(qualified(0, log, test=HOST_TEST))
        self.assertFalse(qualified(101, log, test=HOST_TEST))
        self.assertFalse(qualified(0, log.replace("1 passed", "0 passed"), test=HOST_TEST))

    def test_missing_or_ignored_test_does_not_pass(self):
        for code in (0, 1, 101):
            for negative in (False, True):
                self.assertFalse(qualified(code, "", negative=negative))
                self.assertFalse(qualified(code,
                    f"{TEST}\ntest result: ok. 0 passed; 0 failed; 1 ignored;",
                    negative=negative))


class CacheProbeAdmission(unittest.TestCase):
    def log(self):
        return (f"test {CACHE_TEST} ... ok\nCACHE_MSAA_OPERATOR_PASS\n"
                "test result: ok. 1 passed; 0 failed; 0 ignored;\n")

    def test_cache_requires_its_own_test_and_completion_marker(self):
        log = self.log()
        self.assertTrue(qualified(0, log, test=CACHE_TEST))
        for broken in (log.replace(CACHE_TEST, TEST), log.replace("CACHE_MSAA_OPERATOR_PASS", ""),
                       log.replace("1 passed", "0 passed"), log.replace("0 ignored", "1 ignored"),
                       "compile error E0308", "no adapter", ""):
            self.assertFalse(qualified(0, broken, test=CACHE_TEST))
        self.assertFalse(qualified(101, log, test=CACHE_TEST))

    def test_integration_command_and_missing_evidence_fail_closed(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            with patch("qualify_gpu_operator.subprocess.run",
                       return_value=SimpleNamespace(returncode=0, stdout=self.log())) as run:
                result = run_stage(root, root, "cache", False,
                                   test=CACHE_TEST, integration=CACHE_TARGET)
            command = run.call_args.args[0]
            self.assertIn("--test", command)
            self.assertNotIn("--lib", command)
            self.assertIn(CACHE_TARGET, command)
            self.assertIn("--exact", command)
            self.assertIn("--ignored", command)
            self.assertFalse(result["accepted"], "logs alone are not readback evidence")
            self.assertEqual(result["artifact_sha256"], {})

    def test_missing_cargo_is_failed_attempt_with_retained_log(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            with patch("qualify_gpu_operator.subprocess.run", side_effect=OSError("no cargo")):
                result = run_stage(root, root, "cache", False,
                                   test=CACHE_TEST, integration=CACHE_TARGET)
            self.assertFalse(result["accepted"])
            self.assertEqual(result["return_code"], -1)
            self.assertIn("no cargo", (root / "cache.log").read_text())

    def test_each_attempt_uses_new_artifact_directory(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            with patch("qualify_gpu_operator.subprocess.run",
                       return_value=SimpleNamespace(returncode=0, stdout=self.log())):
                first = run_stage(root, root, "first", False,
                                  test=CACHE_TEST, integration=CACHE_TARGET)
                (root / first["artifact_directory"] / "PASS.txt").write_text("old pass")
                second = run_stage(root, root, "second", False,
                                   test=CACHE_TEST, integration=CACHE_TARGET)
            self.assertNotEqual(first["artifact_directory"], second["artifact_directory"])
            self.assertFalse(second["accepted"])


class CacheReadbackAdmission(unittest.TestCase):
    """Synthetic files test the validator, never GPU execution."""
    def fixture(self, root):
        (root / "PASS.txt").write_text("CACHE_MSAA_OPERATOR_PASS: synthetic parser fixture")
        (root / "profile.txt").write_text("synthetic parser fixture samples=4")
        for name, size, equal in cache_evidence_cases():
            expected = bytearray(size)
            if name == "seed-samples":
                for y in range(8):
                    for x in range(256):
                        for sample in range(4):
                            alpha = (x + sample * 37 + y * 13) % 256
                            offset = ((y * 256 + x) * 4 + sample) * 4
                            expected[offset:offset + 4] = bytes((
                                alpha, x & alpha, sample * 17 % (alpha + 1), alpha))
            actual = expected.copy()
            mask = bytearray(size // 4)
            if not equal:
                actual[0], mask[0] = 1, 255
            (root / f"{name}-expected.rgba").write_bytes(expected)
            (root / f"{name}-actual.rgba").write_bytes(actual)
            (root / f"{name}-diff.mask").write_bytes(mask)

    def test_inventory_is_complete_and_missing_or_truncated_readback_fails(self):
        self.assertEqual(len(cache_evidence_cases()), 57)
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            self.fixture(root)
            self.assertIsNone(cache_evidence_error(root))
            path = root / "round-7-samples-actual.rgba"
            path.write_bytes(b"truncated")
            self.assertIsNotNone(cache_evidence_error(root))
            path.unlink()
            self.assertIsNotNone(cache_evidence_error(root))

    def test_ineffective_negative_control_and_incorrect_mask_fail(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            self.fixture(root)
            path = root / "b0-permuted-diff.mask"
            mask = path.read_bytes()
            path.write_bytes(bytes(len(mask)))
            self.assertIsNotNone(cache_evidence_error(root))
            path.write_bytes(mask)
            expected = (root / "b0-permuted-expected.rgba").read_bytes()
            (root / "b0-permuted-actual.rgba").write_bytes(expected)
            self.assertIsNotNone(cache_evidence_error(root))

    def test_self_consistent_but_wrong_seed_is_rejected(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            self.fixture(root)
            for suffix in ("expected", "actual"):
                path = root / f"seed-samples-{suffix}.rgba"
                path.write_bytes(bytes(len(path.read_bytes())))
            self.assertIsNotNone(cache_evidence_error(root))


if __name__ == "__main__":
    unittest.main()
