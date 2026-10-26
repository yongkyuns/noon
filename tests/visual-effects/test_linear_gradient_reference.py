"""Finite linear-paint oracle and seeded-defect checks; not GPU qualification."""
import math
from pathlib import Path
import unittest

from reference import linear_gradient_pixel, paint_transition_pixel

RED = (1, 0, 0, 1)
BLUE = (0, 0, 1, 1)
STOPS = [(0, RED), (1, BLUE)]


class LinearGradientReference(unittest.TestCase):
    def test_shared_temporal_vectors_and_seeded_defects(self):
        vectors = []
        def numbers(field):
            return tuple(float(v) for v in field.split(","))
        for line in Path(__file__).with_name("paint-transition-vectors.txt").read_text().splitlines():
            if not line or line.startswith("#"):
                continue
            name, a, b, source, target, time, point, opacity, expected = line.split("|")
            actual = paint_transition_pixel(numbers(a), numbers(b),
                [numbers(s) for s in source.split(";")], [numbers(s) for s in target.split(";")],
                float(time), numbers(point), numbers(opacity))
            expected = numbers(expected)
            for x, y in zip(actual, expected):
                self.assertAlmostEqual(x, y, places=14, msg=name)
            vectors.append((name, actual))
        self.assertEqual(len(vectors), 12)
        values = dict(vectors)
        # Straight-alpha temporal mixing leaks invisible red; a frozen source
        # misses time; applying fill opacity twice darkens the independent case.
        self.assertNotEqual(values["transparent_rgb"], (0.25, 0, 0.25, 0.5))
        self.assertNotEqual(values["quarter"], (1, 0, 0, 1))
        self.assertNotEqual(values["independent_opacity"], (0.09375, 0.09375, 0.0625, 0.25))

    def test_shared_rust_vectors_have_independently_recomputed_expected_values(self):
        path = Path(__file__).with_name("linear-gradient-vectors.txt")
        names = set()
        for line in path.read_text().splitlines():
            if not line or line.startswith("#"):
                continue
            name, start, end, stops, point, expected = line.split("|")
            self.assertNotIn(name, names)
            names.add(name)
            def numbers(field):
                return tuple(float(v) for v in field.split(","))
            stops = [numbers(stop) for stop in stops.split(";")]
            actual = linear_gradient_pixel(numbers(start), numbers(end),
                                           [(s[0], s[1:]) for s in stops], numbers(point))
            self.assertEqual(actual, numbers(expected), name)
        self.assertEqual(len(names), 25)

    def sample(self, point, *, start=(0, 0), end=(1, 0), stops=STOPS, **kwargs):
        return linear_gradient_pixel(start, end, stops, point, **kwargs)

    def test_literal_axis_and_color_vectors(self):
        for t in [0, 0.25, 0.5, 0.75, 1]:
            expected = (1 - t, 0, t, 1)
            self.assertEqual(self.sample((t, 0.9)), expected)
            self.assertEqual(self.sample((0.9, t), end=(0, 1)), expected)
            self.assertEqual(self.sample((t, t), end=(1, 1)), expected)
        self.assertEqual(self.sample((1, 0), end=(1, 1)), (0.5, 0, 0.5, 1))
        self.assertEqual(self.sample((0.25, 0), start=(1, 0), end=(0, 0)),
                         (0.25, 0, 0.75, 1))

    def test_premultiplication_and_separate_coverage_opacities(self):
        transparent = [(0, (1, 0, 0, 0)), (1, BLUE)]
        self.assertEqual(self.sample((0.5, 0), stops=transparent), (0, 0, 0.5, 0.5))
        self.assertEqual(self.sample((0.5, 0), stops=transparent, coverage=0.5,
                                     fill_opacity=0.5, object_opacity=0.5),
                         (0, 0, 1 / 16, 1 / 16))

    def test_hard_stops_and_pad_are_exact(self):
        stops = [(0.25, RED), (0.5, RED), (0.5, BLUE), (0.75, BLUE)]
        for x in [-10, 0, 0.25, 0.499]:
            self.assertEqual(self.sample((x, 0), stops=stops), RED)
        for x in [0.5, 0.75, 1, 10]:
            self.assertEqual(self.sample((x, 0), stops=stops), BLUE)

    def test_zero_coverage_and_opacity_do_not_leave_rgb(self):
        for key in ["coverage", "fill_opacity", "object_opacity"]:
            self.assertEqual(self.sample((0.3, 0), **{key: 0}), (0, 0, 0, 0))

    def test_pad_precedes_the_duplicate_zero_edge(self):
        stops = [(0, RED), (0, BLUE), (1, BLUE)]
        self.assertEqual(self.sample((-0.1, 0), stops=stops), RED)
        self.assertEqual(self.sample((0, 0), stops=stops), BLUE)

    def test_coordinate_scaling_and_sample_order_do_not_change_local_paint(self):
        # Recover local coordinates from distinct physical boxes. The paint
        # oracle itself never receives camera zoom or output-resolution state.
        expected = self.sample((0.25, 0.75))
        for origin, size in [((0, 0), (100, 200)), ((30, -40), (400, 800))]:
            point = tuple(origin[i] + (0.25, 0.75)[i] * size[i] for i in range(2))
            local = tuple((point[i] - origin[i]) / size[i] for i in range(2))
            self.assertEqual(self.sample(local), expected)
        literal_red = {0.7: 0.3, 1: 0, 0.2: 0.8, 0: 1}
        for x in [0.7, 1, 0.2, 0.7, 0]:
            self.assertEqual(self.sample((x, 0)), (literal_red[x], 0, x, 1))

    def test_negative_controls_reject_wrong_units_alpha_edges_and_projection(self):
        cases = [
            (self.sample((1, 0), end=(1, 1)), BLUE),  # use x, not projection
            (self.sample((0.25, 0)), self.sample((25, 0))),  # physical px as UV
            (self.sample((0.5, 0), stops=[(0, (1, 0, 0, 0)), (1, BLUE)]),
             (0.25, 0, 0.25, 0.5)),  # interpolate straight RGB, then multiply
            (self.sample((0.5, 0), object_opacity=0.5), (0.125, 0, 0.125, 0.25)),
            (self.sample((0.5, 0), stops=[(0.5, RED), (0.5, BLUE)]), RED),
        ]
        for correct, defective in cases:
            with self.subTest(correct=correct, defective=defective):
                self.assertNotEqual(correct, defective)

    def test_invalid_inputs_fail_closed(self):
        for value in [True, "0.5", math.nan, math.inf, -math.inf]:
            with self.subTest(value=value), self.assertRaises(ValueError):
                self.sample((value, 0))
        for kwargs in [dict(start=(0,)), dict(end=(0, 0)), dict(start=(-1, 0)),
                       dict(stops=[]), dict(stops=[(0, RED)]),
                       dict(stops=[(0.8, RED), (0.2, BLUE)]),
                       dict(stops=[(0, (1, 0, 0)), (1, BLUE)]),
                       dict(stops=[(0, (1, 0, 0, 2)), (1, BLUE)]),
                       dict(stops=[(-1, RED), (1, BLUE)]),
                       dict(coverage=-1), dict(fill_opacity=2), dict(object_opacity=math.nan)]:
            with self.subTest(kwargs=kwargs), self.assertRaises(ValueError):
                self.sample((0.5, 0), **kwargs)


if __name__ == "__main__":
    unittest.main()
