import sys
import types
import unittest

fake_js = types.ModuleType("js")
fake_js.noonResolveAnimationOptions = lambda *args: None
sys.modules["js"] = fake_js

import _manim_compat as manim
import _manim_geometry  # noqa: F401 - installs match_points/layout semantics
import _manim_reactive as reactive
import _manim_updaters as updaters
import noon as api

updaters.install()


class MovingDotsPrimitiveTests(unittest.TestCase):
    def test_value_tracker_reads_callback_runtime_signal_value(self):
        scene = manim.Scene()
        tracker = reactive.value_tracker(scene, 0.0)
        reactive._enter_callback_signal_values(
            {"signals": [{"signal": tracker.signal_id, "value": {"scalar": 2.25}}]}
        )
        try:
            self.assertEqual(tracker.get_value(), 2.25)
        finally:
            reactive._leave_callback_signal_values()
        self.assertEqual(tracker.get_value(), 0.0)

    def test_line_match_points_preserves_source_color(self):
        source = manim.Line((-1.0, 0.0), (1.0, 0.0)).set_color(api.RED)
        target = manim.Line((2.0, 3.0), (4.0, 5.0))
        source.match_points(target)
        self.assertEqual(source.geometry, target.geometry)
        self.assertEqual(source.transform, target.transform)
        self.assertEqual(source.style["stroke"], api.RED.to_ir())

    def test_updater_patch_batch_contains_geometry_replacement(self):
        scene = manim.Scene()
        line = manim.Line((-1.0, 0.0), (1.0, 0.0)).set_color(api.RED)
        scene.add(line)
        line.add_updater(lambda mob: mob.match_points(manim.Line((0.0, 0.0), (2.0, 1.0))))
        registration = updaters.register_scene(scene)
        frame = {
            "time": 0.5,
            "delta_time": 0.5,
            "signals": [],
            "objects": [
                {
                    "object": line.id,
                    "transform": line.transform,
                    "style": line.style,
                    "presence": True,
                    "appearance": 1.0,
                    "reveal": 1.0,
                    "morph": 0.0,
                }
            ],
            "invocations": [{"callback": 0, "object_indices": [0]}],
        }
        import json
        batch = json.loads(updaters.run_callback_phase(registration["session_id"], frame, 0))
        self.assertIn("set_geometry", batch["patches"][0])


if __name__ == "__main__":
    unittest.main()
