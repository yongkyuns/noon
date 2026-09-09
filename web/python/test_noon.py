"""The Python facade cannot substitute a document engine for Rust semantics."""
import math
import unittest
import _noon_ir
from noon import Circle, Color, Scene, VectorPath


class SceneBoundaryTests(unittest.TestCase):
    def test_no_python_scene_or_timeline_authority(self):
        self.assertFalse(hasattr(_noon_ir, "Scene"))
        scene = Scene()
        for name in ("_tracks", "_cursor", "_next_painter_order", "_scheduled_transform_targets",
                     "to_document", "to_json", "identity_document", "animate_position"):
            self.assertFalse(hasattr(scene, name), name)
        for operation in (lambda: scene.add(Circle()), lambda: scene.play(object()),
                          lambda: scene.wait(), lambda: scene.time):
            with self.assertRaisesRegex(RuntimeError, "shared Rust"):
                operation()

    def test_non_finite_color_is_rejected(self):
        with self.assertRaises(ValueError):
            Color(math.inf, 0, 0)

    def test_path_commands_remain_inert_language_input(self):
        path = VectorPath().move_to((0, 0)).line_to((1, 0)).quadratic_to((2, 1), (1, 2))
        path.cubic_to((0, 2), (-1, 1), (0, 0)).close()
        self.assertEqual([next(iter(command)) if isinstance(command, dict) else command
                          for command in path.to_ir()["commands"]],
                         ["move_to", "line_to", "quadratic_to", "cubic_to", "close"])
