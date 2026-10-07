"""Specification/oracle tests only. M1+ must compare real engine observations."""
import ast
import json
import math
import re
from pathlib import Path
import unittest

from reference import (
    assert_field, convolve, glow_pixel, number, over, radius_pixels, restore, sample, smooth,
)

HERE = Path(__file__).resolve().parent
VECTORS = json.loads((HERE / "vectors.json").read_text())
SIGMA = 1 / math.sqrt(2 * math.log(2))


def impulse(size=7, x=3, y=3):
    field = [[0.0] * size for _ in range(size)]
    field[y][x] = 1.0
    return field


def expected_impulse():
    # The rational integers are hand-derived in vectors.json, not obtained by
    # invoking the implementation or production shader being tested.
    weights = VECTORS["kernel_numerators"]
    denominator = VECTORS["impulse_denominator"]
    return [[x * y / denominator for x in weights] for y in weights]


class OperatorContract(unittest.TestCase):
    def test_hand_derived_impulse(self):
        self.assertEqual(sum(VECTORS["kernel_numerators"]), VECTORS["kernel_denominator"])
        self.assertEqual(VECTORS["kernel_denominator"] ** 2, VECTORS["impulse_denominator"])
        result = convolve(impulse(), SIGMA)
        assert_field(result, expected_impulse())
        self.assertAlmostEqual(result[3][3], VECTORS["impulse_center_numerator"] /
                               VECTORS["impulse_denominator"], places=14)
        self.assertAlmostEqual(math.fsum(map(math.fsum, result)), 1.0, places=14)

    def test_normalized_constant_and_transparent_border(self):
        result = convolve([[1.0] * 9 for _ in range(9)], SIGMA)
        self.assertEqual(result[4][4], 1.0)
        # Corner sees only nonnegative offsets. Do not renormalize missing taps.
        one_sided = sum(VECTORS["kernel_numerators"][3:]) / VECTORS["kernel_denominator"]
        self.assertAlmostEqual(result[0][0], one_sided ** 2, places=14)

    def test_zero_radius_and_subnormal_limit(self):
        field = impulse()
        self.assertEqual(convolve(field, 0), field)
        self.assertEqual(convolve(field, math.ulp(0.0)), field)

    def test_translation_and_finite_support(self):
        result = convolve(impulse(15, 7, 7), SIGMA)
        shifted = convolve(impulse(15, 8, 7), SIGMA)
        for y in range(15):
            for x in range(14):
                self.assertAlmostEqual(result[y][x], shifted[y][x + 1], places=14)
        self.assertEqual(result[7][3], 0.0)
        self.assertGreater(result[7][4], 0.0)
        self.assertEqual(result[7][11], 0.0)

    def test_asymmetric_mask_is_sum_of_independent_impulses(self):
        mask = impulse(11, 4, 5)
        mask[6][5] = 0.5
        expected = [[0.0] * 11 for _ in range(11)]
        for cx, cy, scale in [(4, 5, 1.0), (5, 6, 0.5)]:
            for y, row in enumerate(expected_impulse()):
                for x, value in enumerate(row):
                    expected[cy + y - 3][cx + x - 3] += scale * value
        assert_field(convolve(mask, SIGMA), expected)

    def test_units_resolution_and_zoom(self):
        self.assertEqual(radius_pixels(0.15, "scene", 600, 6), 15)
        self.assertEqual(radius_pixels(0.15, "scene", 1200, 6), 30)
        self.assertEqual(radius_pixels(0.15, "scene", 600, 3), 30)
        for height, view in [(600, 6), (1200, 6), (600, 3)]:
            self.assertEqual(radius_pixels(12, "pixels", height, view), 12)

    def test_composition_vector(self):
        c = VECTORS["composite"]
        self.assertEqual(glow_pixel(c["source"], c["mask"], c["tint"], c["intensity"],
                                    c["opacity"]), tuple(c["expected"]))

    def test_neutral_and_opaque_source_identity(self):
        source = (0.25, 0.125, 0.0, 0.5)
        self.assertEqual(glow_pixel(source, 1, (1, 1, 1, 1), 0), source)
        self.assertEqual(glow_pixel(source, 1, (1, 1, 1, 0), 8), source)
        self.assertEqual(glow_pixel((1, 0, 0, 1), 1, (0, 0, 1, 1), 8), (1, 0, 0, 1))
        self.assertEqual(glow_pixel(source, 1, (1, 1, 1, 1), 8, 0), (0, 0, 0, 0))

    def test_transparent_silhouette_can_contribute(self):
        self.assertEqual(glow_pixel((0, 0, 0, 0), 0, (0, 1, 0, 1), 1), (0, 0, 0, 0))
        self.assertEqual(glow_pixel((0, 0, 0, 0), 0.5, (0, 1, 0, 1), 1), (0, 0.5, 0, 0.5))

    def test_group_filter_is_not_per_child(self):
        leaf = (0.25, 0, 0, 0.5)
        group = over(leaf, leaf)
        together = glow_pixel(group, group[3], (0, 0, 1, 1), 0.5)
        filtered = glow_pixel(leaf, leaf[3], (0, 0, 1, 1), 0.5)
        separate = over(filtered, filtered)
        self.assertNotEqual(together, separate)

    def test_order_is_observable(self):
        source = (0, 0, 0, 0)
        red, blue = (1, 0, 0, 1), (0, 0, 1, 1)
        a = glow_pixel(glow_pixel(source, 0.5, red, 1), 0.5, blue, 1)
        b = glow_pixel(glow_pixel(source, 0.5, blue, 1), 0.5, red, 1)
        self.assertNotEqual(a, b)

    def test_reject_invalid_inputs(self):
        for value in [float("nan"), float("inf"), -1, True, "1", 10 ** 1000]:
            with self.subTest(value=value), self.assertRaises(ValueError):
                number(value)
        for field in [[], [[]], [[1], [1, 2]], [[float("nan")]]]:
            with self.subTest(field=field), self.assertRaises(ValueError):
                convolve(field, 1)
        for radius in [-1, 17, float("inf")]:
            with self.subTest(radius=radius), self.assertRaises(ValueError):
                convolve([[1]], radius)
        with self.assertRaises(ValueError):
            radius_pixels(1, "css-pixels", 600, 6)
        with self.assertRaises(ValueError):
            glow_pixel((1, 0, 0, 0), 0, (1, 1, 1, 1), 1)
        with self.assertRaises(ValueError):
            glow_pixel((0, 0, 0, 0), 1, (1, 1, 1, 1), 9)


class TemporalContract(unittest.TestCase):
    def test_linear_activation_and_endpoint_vectors(self):
        for p, expected in VECTORS["linear_samples"]:
            self.assertAlmostEqual(sample(0.35, 1.2, p), expected, places=14)
        # Editing before activation changes the captured start, not the target.
        self.assertEqual(sample(0.9, 1.2, 0), 0.9)

    def test_default_pulse_vectors(self):
        for p, expected in VECTORS["pulse_samples"]:
            self.assertAlmostEqual(sample(0.35, 2, p, pulse=True), expected, places=14)
        self.assertEqual(sample(0, 2, 1, pulse=True), 0)

    def test_smooth_symmetry_and_independent_closed_form(self):
        endpoint = 1 / (1 + math.exp(5))
        for p in [0, 0.125, 0.25, 0.5, 0.9, 1]:
            logistic = (1 / (1 + math.exp(-10 * (p - 0.5))) - endpoint) / (1 - 2 * endpoint)
            self.assertAlmostEqual(smooth(p), logistic, places=14)
            self.assertAlmostEqual(smooth(p) + smooth(1 - p), 1, places=14)

    def test_same_time_does_not_depend_on_sample_order(self):
        times = [0.0, 0.3, 0.5, 1.0, 0.3]
        expected = {p: sample(0.35, 2, p, pulse=True) for p in times}
        for p in reversed(times):
            self.assertEqual(sample(0.35, 2, p, pulse=True), expected[p])

    def test_scoped_restoration_does_not_overwrite_live_edit(self):
        c = VECTORS["supersession"]
        self.assertEqual(restore(c["captured"], c["current"], set(c["still_owned"])), c["expected"])
        with self.assertRaises(ValueError):
            restore(c["captured"], c["current"], {"unknown"})


class NegativeControls(unittest.TestCase):
    def test_field_comparator_rejects_seeded_operator_defects(self):
        correct = expected_impulse()
        mutations = {
            "unnormalized kernel": [[v * 2 for v in row] for row in correct],
            "offset kernel": [row[1:] + [0] for row in correct],
            "cropped halo": [[v if x == 3 and y == 3 else 0 for x, v in enumerate(row)]
                             for y, row in enumerate(correct)],
            "ignored parameter publication": [[0.0] * 7 for _ in range(7)],
            "wrong units / doubled DPR": convolve(impulse(), SIGMA * 2),
        }
        for name, mutated in mutations.items():
            with self.subTest(defect=name), self.assertRaises(AssertionError):
                assert_field(mutated, correct)

    def test_field_comparator_fails_closed(self):
        for field in [[[float("nan")]], [[float("inf")]], [], [[1], [2, 3]]]:
            with self.subTest(field=field), self.assertRaises(ValueError):
                assert_field(field, [[0]])
        with self.assertRaises(AssertionError):
            assert_field([[0, 0]], [[0]])
        with self.assertRaises(ValueError):
            assert_field([[0]], [[0]], float("nan"))

    def test_alpha_and_color_defects_fail_same_comparator(self):
        expected = [VECTORS["composite"]["expected"]]
        for mutated in [
            [[0.0625, 0, 0.125, 0.375]],  # double-premultiplied source
            [[0.125, 0, 0.25, 0.375]],    # straight-alpha halo composition
            [[0.125, 0, 0.125, 0.75]],   # alpha not faded with whole scope
            [[0.046875, 0, 0.046875, 0.375]],  # illicit extra color transfer
        ]:
            with self.subTest(mutated=mutated), self.assertRaises(AssertionError):
                assert_field(mutated, expected)

    def test_frame_count_and_stale_restoration_fail_expected_values(self):
        expected = [[sample(0.35, 2, 0.3, pulse=True)]]
        # The 4th rendered frame need not be authored t=4/60; a dropped display
        # frame cannot select another effect phase.
        with self.assertRaises(AssertionError):
            assert_field([[sample(0.35, 2, 4 / 60, pulse=True)]], expected)
        with self.assertRaises(AssertionError):
            assert_field([[0.35]], [[VECTORS["supersession"]["expected"]["intensity"]]])

    def test_oracle_has_no_product_imports(self):
        tree = ast.parse((HERE / "reference.py").read_text())
        allowed = {"__future__", "math", "collections.abc"}
        for node in ast.walk(tree):
            if isinstance(node, ast.Import):
                self.assertTrue(all(alias.name in allowed for alias in node.names))
            if isinstance(node, ast.ImportFrom):
                self.assertIn(node.module, allowed)


class FixtureIntegrity(unittest.TestCase):
    def test_python_review_specimens_parse_without_importing_proposed_api(self):
        blocks = re.findall(r"```python\n(.*?)```", (HERE / "README.md").read_text(), re.S)
        self.assertEqual(len(blocks), 2)
        for code in blocks:
            tree = ast.parse(code)
            self.assertTrue(any(isinstance(node, ast.ClassDef) for node in tree.body))

    def test_qualification_manifest_is_explicitly_not_measurement_evidence(self):
        protocol = json.loads((HERE / "qualification.json").read_text())
        self.assertEqual(protocol["contract"], VECTORS["contract"])
        self.assertEqual(protocol["schema"], 1)
        self.assertEqual(protocol["quality"]["scale"], 1)
        self.assertFalse(protocol["quality"]["adaptive"])
        self.assertEqual(protocol["cohort"]["pairs"],
                         [["baseline", "candidate"], ["candidate", "baseline"],
                          ["baseline", "candidate"]])
        self.assertFalse(protocol["cohort"]["selective_retries"])
        self.assertTrue(protocol["targets"])
        self.assertTrue(all(target["qualified"] is False for target in protocol["targets"]))
        self.assertEqual({case["id"] for case in protocol["corpus"]},
                         {f"G{i}" for i in range(7)})


if __name__ == "__main__":
    unittest.main()
