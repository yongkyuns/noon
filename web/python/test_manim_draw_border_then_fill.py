import ast
import unittest
from pathlib import Path


class ManimDrawBorderThenFillSourceTests(unittest.TestCase):
    def test_adapter_contains_no_snapshot_or_scheduler_authority(self) -> None:
        source = (Path(__file__).parent / "_manim_draw_border_then_fill.py").read_text()
        tree = ast.parse(source)
        functions = {
            node.name for node in ast.walk(tree) if isinstance(node, ast.FunctionDef)
        }
        self.assertFalse(
            functions
            & {
                "_outline_snapshot",
                "_add_transform_track",
                "_schedule_draw_border_then_fill",
                "_resolved_run_time",
                "_composition_play_leaf",
                "_record_composition_wrapper_state",
            }
        )
        self.assertNotIn("_snapshot_for_object_at", source)
        self.assertNotIn("copy.deepcopy", source)

    def test_canonical_adapter_routes_leaf_and_family_requests(self) -> None:
        source = (Path(__file__).parent / "_manim_canonical_scene.py").read_text()
        self.assertIn("appendDrawBorderThenFillMobject", source)
        self.assertIn("appendDrawBorderThenFillFamily", source)
        self.assertIn("appendDrawBorderThenFillFamilyEntering", source)

    def test_reused_fade_identity_is_not_encoded_as_new_admission(self) -> None:
        source = (Path(__file__).parent / "_manim_canonical_scene.py").read_text()
        self.assertIn("if not reservation.reuse_existing_identity:\n            next_object_id += 1", source)
        self.assertIn(
            'if reservation.reuse_existing_identity\n                    else str(reservation.object.id)',
            source,
        )


if __name__ == "__main__":
    unittest.main()
