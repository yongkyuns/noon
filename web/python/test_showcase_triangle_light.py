"""Source/math/storyboard checks; not a substitute for actual Noon rendering."""
import ast
import copy
import json
import math
from pathlib import Path
import struct
import unittest
import zlib

SOURCE = Path(__file__).parent / "examples/showcase_triangle_light.py"
TREE = ast.parse(SOURCE.read_text())
# Exercise the actual example algorithms without an installed renderer.
NODES = [node for node in TREE.body if isinstance(node, (ast.Import, ast.ImportFrom, ast.FunctionDef, ast.Assign))
         and not (isinstance(node, ast.ImportFrom) and node.module == "noon")]
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


def decode_png(data):
    """Independently check the tiny RGBA encoder, including chunk CRCs."""
    if data[:8] != b"\x89PNG\r\n\x1a\n":
        raise ValueError("invalid PNG signature")
    pos, compressed, dimensions = 8, bytearray(), None
    while pos < len(data):
        length = struct.unpack(">I", data[pos:pos + 4])[0]
        kind = data[pos + 4:pos + 8]
        body = data[pos + 8:pos + 8 + length]
        crc = struct.unpack(">I", data[pos + 8 + length:pos + 12 + length])[0]
        if crc != zlib.crc32(kind + body) & 0xffffffff:
            raise ValueError("bad PNG CRC")
        if kind == b"IHDR":
            w, h, depth, color, compression, filtering, interlace = struct.unpack(">2I5B", body)
            if (depth, color, compression, filtering, interlace) != (8, 6, 0, 0, 0):
                raise ValueError("unsupported PNG format")
            dimensions = (w, h)
        elif kind == b"IDAT":
            compressed.extend(body)
        pos += length + 12
    w, h = dimensions
    raw = zlib.decompress(compressed)
    if len(raw) != h * (w * 4 + 1) or any(raw[row * (w * 4 + 1)] for row in range(h)):
        raise ValueError("invalid scanlines")
    pixels = b"".join(raw[row * (w * 4 + 1) + 1:(row + 1) * (w * 4 + 1)] for row in range(h))
    return w, h, pixels


def recorded_storyboard():
    """Execute ONLY timing/loop syntax; never mock or claim renderer semantics."""
    scene = next(n for n in TREE.body if isinstance(n, ast.ClassDef))
    construct = next(n for n in scene.body if isinstance(n, ast.FunctionDef) and n.name == "construct")
    keep_assignments = {"visible_count", "points", "pngs", "section_points", "checkpoints"}
    def prune(statements):
        output = []
        for node in statements:
            if isinstance(node, ast.Assign):
                names = {n.id for n in ast.walk(node.targets[0]) if isinstance(n, ast.Name)}
                if names & keep_assignments:
                    if "pngs" in names:
                        output.append(ast.Assign(targets=[ast.Name(id="pngs", ctx=ast.Store())],
                                                 value=ast.Call(func=ast.Name(id="range", ctx=ast.Load()), args=[ast.Call(func=ast.Name(id="len", ctx=ast.Load()),args=[ast.Name(id="checkpoints", ctx=ast.Load())],keywords=[])],keywords=[])))
                    else:
                        output.append(copy.deepcopy(node))
            elif isinstance(node, (ast.For, ast.If)):
                body, alternate = prune(node.body), prune(node.orelse)
                if body or alternate:
                    copied = copy.deepcopy(node)
                    copied.body, copied.orelse = body or [ast.Pass()], alternate
                    output.append(copied)
            elif (isinstance(node, ast.Expr) and isinstance(node.value, ast.Call)
                  and isinstance(node.value.func, ast.Attribute)
                  and isinstance(node.value.func.value, ast.Name) and node.value.func.value.id == "self"
                  and node.value.func.attr in ("play", "wait")):
                call = node.value
                duration = call.args[0] if call.func.attr == "wait" else next(k.value for k in call.keywords if k.arg == "run_time")
                output.append(ast.Expr(value=ast.Call(func=ast.Name(id="record", ctx=ast.Load()),
                    args=[ast.Constant(call.func.attr), copy.deepcopy(duration)], keywords=[])))
        return output
    times, holds, clock = [], [], 0.0
    def record(kind, duration):
        nonlocal clock
        if kind == "wait":
            holds.append([clock, clock + duration])
        times.append((kind, duration))
        clock += duration
    namespace = {**MODEL, "record": record}
    tree = ast.fix_missing_locations(ast.Module(body=prune(construct.body), type_ignores=[]))
    exec(compile(tree, str(SOURCE), "exec"), namespace)
    return clock, holds, times


class TriangleLightTests(unittest.TestCase):
    def test_collision_count_and_determinism(self):
        first = trace(TRIANGLE, ORIGIN, ANGLE, MODEL["BOUNCES"])
        self.assertEqual(first, trace(TRIANGLE, ORIGIN, ANGLE, MODEL["BOUNCES"]))
        self.assertEqual(len(first[0]), MODEL["BOUNCES"] + 1)
        self.assertEqual(len(first[1]), MODEL["BOUNCES"])
        self.assertEqual(first[0][0], ORIGIN)

    def test_reflection_law_and_confinement_for_entire_dense_trajectory(self):
        count = MODEL["BOUNCES"] + MODEL["COLOR_BOUNCES"] + MODEL["DENSE_BOUNCES"]
        points, walls = trace(TRIANGLE, ORIGIN, ANGLE, count)
        for index, (point, wall) in enumerate(zip(points[1:], walls), 1):
            a, b = TRIANGLE[wall], TRIANGLE[(wall + 1) % 3]
            tangent = unit(a, b)
            normal = (-tangent[1], tangent[0])
            self.assertAlmostEqual(dot((point[0] - a[0], point[1] - a[1]), normal), 0, places=9)
            for vertex, next_vertex in zip(TRIANGLE, TRIANGLE[1:] + TRIANGLE[:1]):
                self.assertGreaterEqual(MODEL["cross"](MODEL["difference"](next_vertex, vertex), MODEL["difference"](point, vertex)), -1e-9)
            if index < len(points) - 1:
                incoming, outgoing = unit(points[index - 1], point), unit(point, points[index + 1])
                self.assertAlmostEqual(dot(incoming, tangent), dot(outgoing, tangent), places=8)
                self.assertAlmostEqual(dot(incoming, normal), -dot(outgoing, normal), places=8)
                self.assertNotEqual(walls[index - 1], walls[index])

    def test_winding_independence(self):
        a, _ = trace(TRIANGLE, ORIGIN, ANGLE, 100)
        b, _ = trace(tuple(reversed(TRIANGLE)), ORIGIN, ANGLE, 100)
        for left, right in zip(a, b):
            self.assertLess(math.dist(left, right), 1e-8)

    def test_constant_speed_within_each_authored_flight(self):
        points, _ = trace(TRIANGLE, ORIGIN, ANGLE, MODEL["BOUNCES"] + MODEL["COLOR_BOUNCES"])
        for path, seconds in ((points[:MODEL["BOUNCES"] + 1], MODEL["FLIGHT_SECONDS"]),
                              (points[MODEL["BOUNCES"]:], MODEL["COLOR_FLIGHT_SECONDS"])):
            timing = durations(path, seconds)
            self.assertAlmostEqual(math.fsum(timing), seconds, places=12)
            speeds = [math.dist(a, b) / dt for a, b, dt in zip(path, path[1:], timing)]
            self.assertGreater(min(timing), 0)
            for speed in speeds:
                self.assertAlmostEqual(speed, speeds[0], places=10)

    def test_corner_hit_is_explicit_not_arbitrarily_reflected(self):
        vertex = TRIANGLE[2]
        angle = math.degrees(math.atan2(vertex[1] - ORIGIN[1], vertex[0] - ORIGIN[0]))
        with self.assertRaisesRegex(ValueError, "corner.*bounce 1.*LAUNCH_ANGLE"):
            trace(TRIANGLE, ORIGIN, angle, 10)

    def test_parallel_to_base_still_hits_another_mirror(self):
        points, _ = trace(TRIANGLE, ORIGIN, 0.0, 20)
        self.assertEqual(len(points), 21)
        self.assertAlmostEqual(points[1][1], ORIGIN[1])

    def test_invalid_inputs_have_actionable_messages(self):
        cases = [(TRIANGLE, ORIGIN, ANGLE, 0, "BOUNCES"), (TRIANGLE, ORIGIN, ANGLE, 2.5, "BOUNCES"),
                 (TRIANGLE, ORIGIN, ANGLE, True, "BOUNCES"), (TRIANGLE, (20, 20), ANGLE, 2, "strictly inside"),
                 (TRIANGLE, TRIANGLE[0], ANGLE, 2, "strictly inside"),
                 (((0, 0), (1, 0), (2, 0)), ORIGIN, ANGLE, 2, "non-zero area"),
                 (TRIANGLE, ORIGIN, float("nan"), 2, "finite"),
                 (TRIANGLE, (float("inf"), 0), ANGLE, 2, "finite"), (TRIANGLE[:2], ORIGIN, ANGLE, 2, "three 2D")]
        for triangle, origin, angle, count, message in cases:
            with self.subTest(message=message, count=count):
                with self.assertRaisesRegex(ValueError, message):
                    trace(triangle, origin, angle, count)
        for seconds in (0, -1, float("nan"), float("inf")):
            with self.assertRaisesRegex(ValueError, "FLIGHT_SECONDS"):
                durations([(0, 0), (1, 0)], seconds)
        for path in ([], [(0, 0)], [(0, 0), (0, 0)]):
            with self.assertRaisesRegex(ValueError, "segments"):
                durations(path, 1)

    def test_reference_mirror_is_scalene_large_and_right_aligned(self):
        sides = [math.dist(TRIANGLE[i], TRIANGLE[(i + 1) % 3]) for i in range(3)]
        self.assertEqual(len({round(side, 2) for side in sides}), 3)
        self.assertGreater(sum(v[0] for v in TRIANGLE) / 3, 1)
        self.assertGreater(max(v[0] for v in TRIANGLE) - min(v[0] for v in TRIANGLE), 6)
        left_angle = math.degrees(math.atan2(TRIANGLE[2][1] - TRIANGLE[0][1], TRIANGLE[2][0] - TRIANGLE[0][0]))
        self.assertTrue(55 < left_angle < 59)

    def test_storyboard_is_measured_from_actual_source_timing(self):
        manifest = json.loads(SOURCE.with_name("noon_showcase_manifest.json").read_text())
        entry = next(row for row in manifest["entries"] if row["id"] == "showcase-triangle-light")
        duration, holds, timed_calls = recorded_storyboard()
        self.assertAlmostEqual(duration, entry["duration"], places=12)
        self.assertEqual(len(holds), len(entry["still_intervals"]))
        for actual, expected in zip(holds, entry["still_intervals"]):
            for a, b in zip(actual, expected):
                self.assertAlmostEqual(a, b, places=12)
        self.assertTrue(any(a <= entry["thumbnail_time"] <= b for a, b in holds))
        self.assertEqual(len(timed_calls), MODEL["BOUNCES"] + MODEL["COLOR_BOUNCES"] + len(MODEL["EXPOSURE_COUNTS"]) + 5)
        self.assertEqual(entry["path"], "python/examples/" + SOURCE.name)
        self.assertEqual(entry["playback_capability"], "deterministic-retained-replay")

    def test_no_host_frame_callback_or_screenshot_asset(self):
        methods = [node.attr for node in ast.walk(TREE) if isinstance(node, ast.Attribute)]
        for prohibited in ("add_updater", "add_callback", "on_frame"):
            self.assertNotIn(prohibited, methods)
        scene = next(n for n in TREE.body if isinstance(n, ast.ClassDef))
        self.assertFalse(any(isinstance(n, ast.While) for n in ast.walk(scene)))
        self.assertNotIn("random", MODEL)
        self.assertFalse(any(isinstance(n, ast.Constant) and isinstance(n.value, str)
                             and n.value.lower().endswith((".png", ".jpg", ".webp", ".mp4")) for n in ast.walk(scene)))

    def test_direction_ring_counts_the_same_trajectory(self):
        points, _ = trace(TRIANGLE, ORIGIN, ANGLE, 500)
        hist = MODEL["direction_histogram"](points)
        self.assertEqual(len(hist), MODEL["RING_BINS"])
        self.assertEqual(sum(hist), 500)
        self.assertEqual(hist, MODEL["direction_histogram"](points))
        self.assertAlmostEqual(MODEL["direction_hue"]((0, 0), (1, 0)), 0)
        self.assertAlmostEqual(MODEL["direction_hue"]((0, 0), (0, -1)), 0.25)

    def test_png_encoder_round_trip(self):
        pixels = bytes((255, 0, 0, 255, 0, 255, 0, 128))
        self.assertEqual(decode_png(MODEL["rgba_png"](2, 1, pixels)), (2, 1, pixels))
        with self.assertRaisesRegex(ValueError, "dimensions"):
            MODEL["rgba_png"](2, 1, b"bad")

    def test_exposures_are_deterministic_increasing_and_triangle_clipped(self):
        points, _ = trace(TRIANGLE, ORIGIN, ANGLE, 200)
        make = MODEL["accumulate_ray_exposures"]
        images, bounds = make(TRIANGLE, points, (4, 50, 200), width=64)
        self.assertEqual(images, make(TRIANGLE, points, (4, 50, 200), width=64)[0])
        self.assertEqual(len(set(images)), 3)
        decoded = [decode_png(image) for image in images]
        w, h, last = decoded[-1]
        xmin, ymin, xmax, ymax = bounds
        for row in range(h):
            for col in range(w):
                i = (row * w + col) * 4
                point = (xmin + (col + 0.5) * (xmax - xmin) / w, ymax - (row + 0.5) * (ymax - ymin) / h)
                inside = all(MODEL["cross"](MODEL["difference"](TRIANGLE[(n + 1) % 3], TRIANGLE[n]), MODEL["difference"](point, TRIANGLE[n])) >= 0 for n in range(3))
                self.assertEqual(last[i + 3], 255 if inside else 0)
                for channel in range(3):
                    values = [image[2][i + channel] for image in decoded]
                    self.assertEqual(values, sorted(values))
        sparse = decoded[0][2]
        self.assertGreater(sum(sparse[i:i + 3] == bytes((27, 28, 48)) for i in range(0, len(sparse), 4)), 100)

    def test_exposure_changes_when_the_ray_changes(self):
        a, _ = trace(TRIANGLE, ORIGIN, 17, 100)
        b, _ = trace(TRIANGLE, ORIGIN, 25, 100)
        make = MODEL["accumulate_ray_exposures"]
        self.assertNotEqual(make(TRIANGLE, a, (100,), width=64)[0], make(TRIANGLE, b, (100,), width=64)[0])

    def test_procedural_wood_is_an_opaque_deterministic_resource(self):
        png = MODEL["wood_texture"](32, 18)
        self.assertEqual(png, MODEL["wood_texture"](32, 18))
        width, height, pixels = decode_png(png)
        self.assertEqual((width, height), (32, 18))
        self.assertTrue(all(pixels[i] == 255 for i in range(3, len(pixels), 4)))
        self.assertGreater(len({pixels[i:i + 3] for i in range(0, len(pixels), 4)}), 20)

    def test_exposure_rejects_invalid_controls(self):
        points, _ = trace(TRIANGLE, ORIGIN, ANGLE, 10)
        for checkpoints in ((), (0,), (11,), (5, 4), (4, 4), (True,), (2.5,)):
            with self.assertRaisesRegex(ValueError, "checkpoints"):
                MODEL["accumulate_ray_exposures"](TRIANGLE, points, checkpoints, width=64)
        for width in (0, 15, 1025, 3.2, True):
            with self.assertRaisesRegex(ValueError, "width"):
                MODEL["accumulate_ray_exposures"](TRIANGLE, points, (10,), width=width)


if __name__ == "__main__":
    unittest.main()
