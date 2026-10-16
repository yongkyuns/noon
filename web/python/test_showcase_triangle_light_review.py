"""Review regressions: exact source math and authoring structure, not rendered pixels."""
import ast
import math
from pathlib import Path
import struct
import unittest
import zlib

SOURCE = Path(__file__).parent / "examples/showcase_triangle_light.py"
TREE = ast.parse(SOURCE.read_text())
MODEL = {}
exec(compile(ast.Module(body=[node for node in TREE.body
    if isinstance(node, (ast.Import, ast.ImportFrom, ast.FunctionDef, ast.Assign))
    and not (isinstance(node, ast.ImportFrom) and node.module == "noon")], type_ignores=[]),
    str(SOURCE), "exec"), MODEL)


def rgba(data):
    at, compressed = 8, bytearray()
    while at < len(data):
        size = struct.unpack(">I", data[at:at + 4])[0]
        kind, chunk = data[at + 4:at + 8], data[at + 8:at + 8 + size]
        if kind == b"IHDR":
            width, height = struct.unpack(">II", chunk[:8])
        if kind == b"IDAT":
            compressed.extend(chunk)
        at += size + 12
    raw = zlib.decompress(compressed)
    return width, height, b"".join(raw[row * (width * 4 + 1) + 1:(row + 1) * (width * 4 + 1)]
                                    for row in range(height))


class TriangleLightReviewTests(unittest.TestCase):
    def test_empty_direction_bins_have_no_bar(self):
        self.assertEqual(MODEL["ring_bar_lengths"]([0, 0, 0]), [0, 0, 0])
        self.assertEqual(MODEL["ring_bar_lengths"]([0, 1, 4]), [0, 30.5, 61])
        for invalid in ([], [-1], [True], [1.5]):
            with self.assertRaisesRegex(ValueError, "nonnegative integers"):
                MODEL["ring_bar_lengths"](invalid)

    def test_invalid_histogram_dimensions_fail_early(self):
        for invalid in (0, -1, True, 2.5, 361):
            with self.assertRaisesRegex(ValueError, "histogram bins"):
                MODEL["direction_histogram"]([(0, 0), (1, 0)], invalid)

    def test_physical_wall_is_exact_inner_rim_edge_for_both_windings(self):
        for triangle in (MODEL["TRIANGLE"], tuple(reversed(MODEL["TRIANGLE"])),
                         ((-2, -1), (3, -1), (0.5, 2))):
            outer = MODEL["offset_triangle"](triangle, 0.08)
            for i in range(3):
                a, b = triangle[i], triangle[(i + 1) % 3]
                c, d = outer[i], outer[(i + 1) % 3]
                edge = MODEL["difference"](b, a)
                self.assertAlmostEqual(MODEL["cross"](edge, MODEL["difference"](d, c)), 0, places=12)
                self.assertAlmostEqual(abs(MODEL["cross"](edge, MODEL["difference"](c, a))) / math.hypot(*edge), 0.08, places=12)

    def test_triangle_edit_moves_the_mirror_not_only_the_ray(self):
        triangle = MODEL["TRIANGLE"]
        moved = tuple((x + 1.0, y - 0.5) for x, y in triangle)
        for a, b in zip(MODEL["offset_triangle"](triangle, 0.08), MODEL["offset_triangle"](moved, 0.08)):
            self.assertAlmostEqual(b[0] - a[0], 1)
            self.assertAlmostEqual(b[1] - a[1], -0.5)
        method = next(n for n in TREE.body if isinstance(n, ast.FunctionDef) and n.name == "mirror_artwork")
        self.assertTrue(any(isinstance(n, ast.Name) and n.id == "TRIANGLE" for n in ast.walk(method)))

    def test_subpixel_ray_deposits_nonzero_energy(self):
        triangle = ((0, 0), (1, 0), (0, 1))
        images, _ = MODEL["accumulate_ray_exposures"](triangle, [(0.30, 0.30), (0.301, 0.301)], (1,), width=16)
        _, _, pixels = rgba(images[0])
        self.assertTrue(any(pixels[i + 3] and tuple(pixels[i:i + 3]) != (27, 28, 48)
                            for i in range(0, len(pixels), 4)))

    def test_extreme_exposure_aspect_ratio_cannot_allocate_unbounded_memory(self):
        with self.assertRaisesRegex(ValueError, "too tall"):
            MODEL["accumulate_ray_exposures"](((0, 0), (0.001, 0), (0, 1000)),
                                               [(0, 0), (0, 1)], (1,), width=16)

    def test_png_encoder_rejects_invalid_dimensions(self):
        for width, height in ((0, 1), (1, 0), (-1, 1), (True, 1), (1.2, 1)):
            with self.assertRaisesRegex(ValueError, "dimensions"):
                MODEL["rgba_png"](width, height, b"")

    def test_workload_edits_fail_before_precomputation(self):
        MODEL["validate_showcase_controls"]()
        for name, invalid in (("BOUNCES", 0), ("COLOR_BOUNCES", -1), ("DENSE_BOUNCES", 1000000),
                              ("FLIGHT_SECONDS", 0), ("COLOR_FLIGHT_SECONDS", float("nan")),
                              ("EXPOSURE_SECONDS", float("inf"))):
            old = MODEL[name]
            try:
                MODEL[name] = invalid
                with self.assertRaises(ValueError):
                    MODEL["validate_showcase_controls"]()
            finally:
                MODEL[name] = old

    def test_small_dense_counts_have_valid_unique_checkpoints(self):
        assignment = next(n for n in TREE.body if isinstance(n, ast.Assign)
                          and any(isinstance(t, ast.Name) and t.id == "EXPOSURE_COUNTS" for t in n.targets))
        for count in (1, 2, 7, 10000):
            model = {"DENSE_BOUNCES": count}
            exec(compile(ast.Module(body=[assignment], type_ignores=[]), str(SOURCE), "exec"), model)
            checkpoints = model["EXPOSURE_COUNTS"]
            self.assertEqual(checkpoints, tuple(sorted(set(checkpoints))))
            self.assertTrue(all(1 <= n <= count for n in checkpoints))
            self.assertEqual(checkpoints[-1], count)

    def test_crossfade_does_not_remove_the_opaque_underlay_early(self):
        scene = next(n for n in TREE.body if isinstance(n, ast.ClassDef))
        loop = next(n for n in ast.walk(scene) if isinstance(n, ast.For)
                    and isinstance(n.iter, ast.Call) and ast.unparse(n.iter) == "enumerate(pngs)")
        calls = [n for n in ast.walk(loop) if isinstance(n, ast.Call)]
        self.assertFalse(any(ast.unparse(n) == "FadeOut(previous)" for n in calls))
        play_index = next(i for i, n in enumerate(loop.body) if isinstance(n, ast.Expr)
                          and isinstance(n.value, ast.Call) and ast.unparse(n.value.func) == "self.play")
        self.assertTrue(any(ast.unparse(n) == "self.remove(previous)"
                            for statement in loop.body[play_index + 1:] for n in ast.walk(statement)))
        # Source-over fade-in over an opaque predecessor is affine in energy.
        # Simultaneously fading the predecessor instead exposes the backdrop.
        previous, incoming, background, alpha = 0.7, 0.9, 0.1, 0.5
        correct = alpha * incoming + (1 - alpha) * previous
        old = alpha * incoming + (1 - alpha) * ((1 - alpha) * previous + alpha * background)
        self.assertGreaterEqual(correct, previous)
        self.assertLess(old, previous)

    def test_visible_beam_has_actual_glow_and_photon_hue_is_not_stuck_pink(self):
        calls = {ast.unparse(n) for n in ast.walk(TREE) if isinstance(n, ast.Call)}
        self.assertIn("halo.set_glow(color=color, radius=Pixels(3), intensity=1.0)", calls)
        self.assertIn("photon.set_glow(color=color)", calls)


if __name__ == "__main__":
    unittest.main()
