"""Source-level visibility timing guarantees, distinct from actual GPU frames."""
import ast
import math
from pathlib import Path
import unittest

SOURCE = Path(__file__).parent / "examples/showcase_triangle_light.py"
TREE = ast.parse(SOURCE.read_text())
MODEL = {}
NODES = [node for node in TREE.body if isinstance(node, (ast.Import, ast.ImportFrom, ast.FunctionDef, ast.Assign))
         and not (isinstance(node, ast.ImportFrom) and node.module == "noon")]
exec(compile(ast.Module(body=NODES, type_ignores=[]), str(SOURCE), "exec"), MODEL)


class TriangleLightVisibilityTests(unittest.TestCase):
    def test_first_entire_flight_is_long_enough_for_observable_motion(self):
        points, _ = MODEL["trace_ray"](MODEL["TRIANGLE"], MODEL["LAUNCH_POINT"],
                                      MODEL["LAUNCH_ANGLE_DEGREES"], MODEL["BOUNCES"])
        durations = MODEL["pink_flight_durations"](points, MODEL["FLIGHT_SECONDS"])
        self.assertEqual(len(durations), MODEL["BOUNCES"])
        self.assertAlmostEqual(math.fsum(durations), MODEL["FLIGHT_SECONDS"], places=12)
        self.assertEqual(MODEL["SLOW_BOUNCES"], 4)
        self.assertAlmostEqual(math.fsum(durations[:4]), MODEL["SLOW_FLIGHT_SECONDS"], places=12)
        self.assertGreater(min(durations[:4]), 0.7)
        first_end = 0.8 + durations[0]
        self.assertLess(1.9, first_end, "all three real screenshots must be within bounce one")

    def test_deliberate_speed_change_preserves_constant_speed_between_walls(self):
        points, _ = MODEL["trace_ray"](MODEL["TRIANGLE"], MODEL["LAUNCH_POINT"],
                                      MODEL["LAUNCH_ANGLE_DEGREES"], MODEL["BOUNCES"])
        durations = MODEL["pink_flight_durations"](points, MODEL["FLIGHT_SECONDS"])
        for start, stop in ((0, 4), (4, MODEL["BOUNCES"])):
            velocities = [math.dist(points[i], points[i + 1]) / durations[i]
                          for i in range(start, stop)]
            for value in velocities:
                self.assertAlmostEqual(value, velocities[0], places=9)
        slow_speed = math.dist(points[0], points[1]) / durations[0]
        fast_speed = math.dist(points[5], points[6]) / durations[5]
        self.assertGreater(fast_speed, slow_speed * 5)

    def test_scene_uses_authored_ray_timing_not_frame_callbacks(self):
        scene = next(node for node in TREE.body if isinstance(node, ast.ClassDef))
        self.assertTrue(any(isinstance(node, ast.Call) and isinstance(node.func, ast.Name)
                            and node.func.id == "pink_flight_durations" for node in ast.walk(scene)))
        self.assertFalse(any(isinstance(node, ast.Attribute) and node.attr in ("add_updater", "always_redraw")
                             for node in ast.walk(scene)))


if __name__ == "__main__":
    unittest.main()
