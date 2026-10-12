import importlib.util
import unittest
from pathlib import Path

spec = importlib.util.spec_from_file_location("dispositions", Path(__file__).with_name("manim-reference-dispositions.py"))
dispositions = importlib.util.module_from_spec(spec)
spec.loader.exec_module(dispositions)


class DispositionTests(unittest.TestCase):
    def test_new_export_requires_reconciliation_but_does_not_prove_parity(self):
        entries = {"entries": [{"id": "angle", "status": "blocked", "blocker_kind": "namespace", "namespace_blockers": ["Angle"]}]}
        self.assertEqual(dispositions.validate_namespace_blockers(entries, set()), [])
        errors = dispositions.validate_namespace_blockers(entries, {"Angle"})
        self.assertIn("without assuming parity", errors[0])

    def test_unqualified_record_cannot_disguise_a_namespace_blocker(self):
        entries = {"entries": [{"id": "angle", "status": "unqualified", "namespace_blockers": ["Angle"]}]}
        self.assertTrue(dispositions.validate_namespace_blockers(entries, set()))

    def test_unqualified_and_observed_behavior_failures_are_not_promoted(self):
        entries = {"entries": [{"id": "a", "status": "unqualified"}, {"id": "b", "status": "blocked", "blocker_kind": "behavior"}]}
        self.assertEqual(dispositions.validate_namespace_blockers(entries, {"Angle"}), [])


if __name__ == "__main__":
    unittest.main()
