"""Pure geometry qualification; these tests do not claim renderer validation."""
import ast
import json
import math
from pathlib import Path
import unittest

SOURCE = Path(__file__).parent / "examples/showcase_triangle_light.py"
TREE = ast.parse(SOURCE.read_text())
# The example stays self-contained for the browser editor. Exercise its actual
# math without loading the native/Pyodide wrapper or inventing a second solver.
NODES = [node for node in TREE.body if isinstance(node, (ast.Import, ast.FunctionDef, ast.Assign))]
MODEL = {}
exec(compile(ast.Module(body=NODES, type_ignores=[]), str(SOURCE), "exec"), MODEL)
trace = MODEL["trace_ray"]
durations = MODEL["flight_durations"]
TRIANGLE = MODEL["TRIANGLE"]
ORIGIN = MODEL["LAUNCH_POINT"]
ANGLE = MODEL["LAUNCH_ANGLE_DEGREES"]


def unit(a, b):
    length = math.dist(a, b)
    return ((b[0] - a[0]) / length, (b[1] - a[1]) / length)


def dot(a, b):
    return a[0] * b[0] + a[1] * b[1]


class TriangleLightTests(unittest.TestCase):
    def test_default_collision_count_and_determinism(self):
        first = trace(TRIANGLE, ORIGIN, ANGLE, MODEL["BOUNCES"])
        self.assertEqual(first, trace(TRIANGLE, ORIGIN, ANGLE, MODEL["BOUNCES"]))
        self.assertEqual(len(first[0]), MODEL["BOUNCES"] + 1)
        self.assertEqual(len(first[1]), MODEL["BOUNCES"])
        self.assertEqual(first[0][0], ORIGIN)

    def test_reflection_law_and_inside_triangle_for_long_path(self):
        points, walls = trace(TRIANGLE, ORIGIN, ANGLE, 1000)
        for index, (point, wall) in enumerate(zip(points[1:], walls), 1):
            a, b = TRIANGLE[wall], TRIANGLE[(wall + 1) % 3]
            tangent = unit(a, b)
            normal = (-tangent[1], tangent[0])
            self.assertAlmostEqual(dot((point[0] - a[0], point[1] - a[1]), normal), 0, places=9)
            for vertex, next_vertex in zip(TRIANGLE, TRIANGLE[1:] + TRIANGLE[:1]):
                edge = (next_vertex[0] - vertex[0], next_vertex[1] - vertex[1])
                offset = (point[0] - vertex[0], point[1] - vertex[1])
                self.assertGreaterEqual(MODEL["cross"](edge, offset), -1e-9)
            if index < len(points) - 1:
                incoming, outgoing = unit(points[index - 1], point), unit(point, points[index + 1])
                self.assertAlmostEqual(dot(incoming, tangent), dot(outgoing, tangent), places=9)
                self.assertAlmostEqual(dot(incoming, normal), -dot(outgoing, normal), places=9)
                self.assertNotEqual(walls[index - 1], walls[index])

    def test_winding_independence(self):
        a, _ = trace(TRIANGLE, ORIGIN, ANGLE, 100)
        b, _ = trace(tuple(reversed(TRIANGLE)), ORIGIN, ANGLE, 100)
        for left, right in zip(a, b):
            self.assertLess(math.dist(left, right), 1e-8)

    def test_constant_speed_and_exact_flight_duration(self):
        points, _ = trace(TRIANGLE, ORIGIN, ANGLE, MODEL["BOUNCES"])
        timing = durations(points, MODEL["FLIGHT_SECONDS"])
        self.assertAlmostEqual(math.fsum(timing), MODEL["FLIGHT_SECONDS"], places=12)
        speeds = [math.dist(a, b) / dt for a, b, dt in zip(points, points[1:], timing)]
        for speed in speeds:
            self.assertAlmostEqual(speed, speeds[0], places=10)
        self.assertGreater(min(timing), 0)

    def test_corner_hit_is_explicit_not_arbitrarily_reflected(self):
        vertex = TRIANGLE[2]
        angle = math.degrees(math.atan2(vertex[1] - ORIGIN[1], vertex[0] - ORIGIN[0]))
        with self.assertRaisesRegex(ValueError, "corner.*bounce 1.*LAUNCH_ANGLE"):
            trace(TRIANGLE, ORIGIN, angle, 10)

    def test_parallel_to_base_still_hits_another_mirror(self):
        points, _ = trace(TRIANGLE, ORIGIN, 0.0, 20)
        self.assertEqual(len(points), 21)
        self.assertAlmostEqual(points[1][1], ORIGIN[1])

    def test_different_launch_angles_remain_inside(self):
        for angle in (-343.0, 17.0, 73.0, 141.0, 253.0, 377.0):
            with self.subTest(angle=angle):
                points, _ = trace(TRIANGLE, ORIGIN, angle, 150)
                for p in points:
                    for a, b in zip(TRIANGLE, TRIANGLE[1:] + TRIANGLE[:1]):
                        self.assertGreaterEqual(MODEL["cross"](MODEL["difference"](b, a), MODEL["difference"](p, a)), -1e-9)

    def test_invalid_inputs_fail_with_actionable_messages(self):
        cases = [
            (TRIANGLE, ORIGIN, ANGLE, 0, "BOUNCES"),
            (TRIANGLE, ORIGIN, ANGLE, 2.5, "BOUNCES"),
            (TRIANGLE, ORIGIN, ANGLE, True, "BOUNCES"),
            (TRIANGLE, (20, 20), ANGLE, 2, "strictly inside"),
            (TRIANGLE, TRIANGLE[0], ANGLE, 2, "strictly inside"),
            (((0, 0), (1, 0), (2, 0)), ORIGIN, ANGLE, 2, "non-zero area"),
            (TRIANGLE, ORIGIN, float("nan"), 2, "finite"),
            (TRIANGLE, (float("inf"), 0), ANGLE, 2, "finite"),
            (TRIANGLE[:2], ORIGIN, ANGLE, 2, "three 2D"),
        ]
        for triangle, origin, angle, count, message in cases:
            with self.subTest(message=message, count=count):
                with self.assertRaisesRegex(ValueError, message):
                    trace(triangle, origin, angle, count)
        for seconds in (0, -1, float("nan"), float("inf")):
            with self.assertRaisesRegex(ValueError, "FLIGHT_SECONDS"):
                durations([(0, 0), (1, 0)], seconds)
        for points in ([], [(0, 0)], [(0, 0), (0, 0)]):
            with self.assertRaisesRegex(ValueError, "segments"):
                durations(points, 1)

    def test_candidate_storyboard_matches_flight_and_final_hold(self):
        entry = json.loads(SOURCE.with_suffix(".candidate.json").read_text())
        self.assertEqual(entry["duration"], 1.0 + 0.4 + MODEL["FLIGHT_SECONDS"] + 0.6 + 1.0)
        self.assertEqual(entry["still_intervals"], [[entry["duration"] - 1.0, entry["duration"]]])
        self.assertTrue(entry["duration"] - 1.0 <= entry["thumbnail_time"] <= entry["duration"])
        self.assertEqual(entry["path"], "python/examples/" + SOURCE.name)
        self.assertEqual(entry["thumbnail"], "thumbnails/showcase/" + entry["id"] + ".png")
        self.assertEqual(entry["playback_capability"], "deterministic-retained-replay")
        self.assertGreaterEqual(len(entry["beats"]), 3)
        self.assertTrue(all(0 < beat["time"] <= entry["duration"] for beat in entry["beats"]))

    def test_scene_uses_retained_linear_plays_not_host_callbacks(self):
        methods = [node.attr for node in ast.walk(TREE) if isinstance(node, ast.Attribute)]
        for prohibited in ("add_updater", "add_callback", "on_frame"):
            self.assertNotIn(prohibited, methods)
        self.assertNotIn("while", [type(node).__name__.lower() for node in ast.walk(TREE)])
        linear_plays = [node for node in ast.walk(TREE) if isinstance(node, ast.Call)
                        and isinstance(node.func, ast.Attribute) and node.func.attr == "play"
                        and any(k.arg == "rate_func" and isinstance(k.value, ast.Name) and k.value.id == "linear" for k in node.keywords)]
        self.assertEqual(len(linear_plays), 1)
        self.assertEqual(len(linear_plays[0].args), 2)


if __name__ == "__main__":
    unittest.main()
