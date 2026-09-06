import unittest
from pathlib import Path


class CanonicalCompositionContractTests(unittest.TestCase):
    def test_recursive_frontend_keeps_timing_and_admission_in_shared_tree(self) -> None:
        source = (Path(__file__).resolve().parent / "_manim_canonical_scene.py").read_text()
        self.assertIn("setCompositionRateFunction", source)
        self.assertIn("setPlayRateFunction", source)
        self.assertIn("builder.appendComposition(nested)", source)
        self.assertIn("canonical Add requires one detached typed leaf", source)
        self.assertNotIn("_compat._leaf_mobjects(animation.mobject)", source)
        self.assertNotIn("reservations.clear()", source)

    def test_failed_shared_admission_precedes_wrapper_commit(self) -> None:
        source = (Path(__file__).resolve().parent / "_manim_canonical_scene.py").read_text()
        candidate = source.index("def _play_canonical_composition(")
        commit = source.index("_commit_typed_binding", candidate)
        shared_call = source.index("context.beginOrdinaryComposition(candidate)", candidate)
        self.assertLess(shared_call, commit)


if __name__ == "__main__":
    unittest.main()
