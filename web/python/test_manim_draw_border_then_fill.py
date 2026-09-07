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

if __name__ == "__main__":
    unittest.main()
