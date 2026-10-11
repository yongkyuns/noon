"""Control-flow/parser checks only; these are not GPU execution evidence."""
import unittest

from qualify_gpu_operator import HOST_TEST, ANIMATED_TEST, LIVE_TEST, PAINTER_TEST, SEMANTIC_TEST, TEST, qualified


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


if __name__ == "__main__":
    unittest.main()
