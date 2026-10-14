"""Independent exact-arithmetic witnesses for #2094 M0; not GPU qualification.

This is a four-sample operator reference, not a geometry rasterizer, renderer,
cache implementation, or model of Noon's semantic/publication/lifetime system.
Masks are explicit synthetic sample coverage, not claimed hardware locations.
Passing these tests does NOT establish production cache equivalence or speed.
"""
from fractions import Fraction as F
import ast
import hashlib
from itertools import product
from pathlib import Path
import unittest


ZERO = (F(0), F(0), F(0), F(0))
SAMPLES = 4


def unit(value):
    """Accept exact normalized inputs only; never hide a float tolerance."""
    if isinstance(value, bool) or not isinstance(value, (int, F)):
        raise ValueError("expected an exact integer or Fraction")
    value = F(value)
    if not 0 <= value <= 1:
        raise ValueError("expected a normalized component")
    return value


def rgba(value):
    if len(value) != 4:
        raise ValueError("expected RGBA")
    result = tuple(unit(channel) for channel in value)
    if any(channel > result[3] for channel in result[:3]):
        raise ValueError("expected premultiplied RGBA")
    return result


def premultiply(straight):
    if len(straight) != 4:
        raise ValueError("expected straight RGBA")
    r, g, b, a = (unit(channel) for channel in straight)
    return (r * a, g * a, b * a, a)


def over(source, destination):
    source, destination = rgba(source), rgba(destination)
    return tuple(s + (1 - source[3]) * d for s, d in zip(source, destination))


def fade(pixel, opacity):
    return tuple(channel * unit(opacity) for channel in rgba(pixel))


def unorm8(pixel):
    """Illustrative nearest/half-up store, NOT a GPU rounding specification.

    The associativity counterexample is away from ties. Each stored component
    is represented exactly as an integer code / 255, with no color transfer.
    Device stores, intermediate precision and transfer require GPU evidence.
    """
    result = []
    for channel in rgba(pixel):
        scaled = channel * 255
        code = (2 * scaled.numerator + scaled.denominator) // (2 * scaled.denominator)
        result.append(F(code, 255))
    return tuple(result)


def paint(samples, source, mask, *, quantized=False):
    if len(samples) != SAMPLES:
        raise ValueError("expected four explicit samples")
    if isinstance(mask, bool) or not isinstance(mask, int) or not 0 <= mask < 16:
        raise ValueError("expected a four-bit coverage mask")
    source = rgba(source)
    result = []
    for index, destination in enumerate(samples):
        destination = rgba(destination)
        if mask & (1 << index):
            destination = over(source, destination)
            if quantized:
                destination = unorm8(destination)
        result.append(destination)
    return tuple(result)


def resolve(samples):
    if len(samples) != SAMPLES:
        raise ValueError("expected four explicit samples")
    samples = tuple(rgba(pixel) for pixel in samples)
    return tuple(sum((pixel[channel] for pixel in samples), F(0)) / SAMPLES
                 for channel in range(4))


class RenderedContentCacheReference(unittest.TestCase):
    def test_hand_computed_source_over(self):
        self.assertEqual(over((F(1, 2), 0, 0, F(1, 2)), (0, 0, F(1, 4), F(1, 4))),
                         (F(1, 2), 0, F(1, 8), F(5, 8)))

    def test_alpha_endpoints_near_zero_and_colored_transparent_input(self):
        backdrops = [ZERO, (0, 0, 0, 1), (1, 1, 1, 1), (0, F(1, 4), F(1, 2), F(1, 2))]
        for backdrop in backdrops:
            with self.subTest(backdrop=backdrop):
                self.assertEqual(over(ZERO, backdrop), backdrop)
                self.assertEqual(over((1, 0, 0, 1), backdrop), (1, 0, 0, 1))
        self.assertEqual(premultiply((1, 0, 1, 0)), ZERO)
        tiny = F(1, 65535)
        self.assertEqual(over((tiny, 0, 0, tiny), (0, 0, 1, 1)),
                         (tiny, 0, 1 - tiny, 1))

    def test_child_opacity_is_not_postcomposed_group_opacity(self):
        red, blue = (1, 0, 0, 1), (0, 0, 1, 1)
        children = over(fade(blue, F(1, 2)), fade(red, F(1, 2)))
        group = fade(over(blue, red), F(1, 2))
        self.assertEqual(children, (F(1, 4), 0, F(1, 2), F(3, 4)))
        self.assertEqual(group, (0, 0, F(1, 2), F(1, 2)))
        self.assertNotEqual(children, group)

    def test_static_dynamic_static_cannot_be_gathered(self):
        red, green, blue = [(F(1, 2), 0, 0, F(1, 2)),
                            (0, F(1, 2), 0, F(1, 2)),
                            (0, 0, F(1, 2), F(1, 2))]
        ordered = over(blue, over(green, red))
        gathered = over(green, over(blue, red))
        self.assertEqual(ordered, (F(1, 8), F(1, 4), F(1, 2), F(7, 8)))
        self.assertEqual(gathered, (F(1, 8), F(1, 2), F(1, 4), F(7, 8)))
        self.assertNotEqual(ordered, gathered)

    def test_coincident_half_coverage_loses_correlation_after_resolve(self):
        prefix = paint((ZERO,) * 4, (1, 0, 0, 1), 0b0011)
        direct = resolve(paint(prefix, (0, 0, 1, 1), 0b0011))
        early = resolve(paint((resolve(prefix),) * 4, (0, 0, 1, 1), 0b0011))
        self.assertEqual(direct, (0, 0, F(1, 2), F(1, 2)))
        self.assertEqual(early, (F(1, 4), 0, F(1, 2), F(3, 4)))
        self.assertNotEqual(direct, early)

    def test_disjoint_half_coverage_loses_correlation_after_resolve(self):
        prefix = paint((ZERO,) * 4, (1, 0, 0, 1), 0b0011)
        direct = resolve(paint(prefix, (0, 0, 1, 1), 0b1100))
        early = resolve(paint((resolve(prefix),) * 4, (0, 0, 1, 1), 0b1100))
        self.assertEqual(direct, (F(1, 2), 0, F(1, 2), 1))
        self.assertEqual(early, (F(1, 4), 0, F(1, 2), F(3, 4)))
        self.assertNotEqual(direct, early)

    def test_crossing_translucent_sample_masks(self):
        # Synthetic crossing: one shared sample, not a geometry/AA oracle.
        prefix = paint((ZERO,) * 4, (F(1, 2), 0, 0, F(1, 2)), 0b0011)
        suffix = (0, 0, F(1, 2), F(1, 2))
        self.assertEqual(resolve(paint(prefix, suffix, 0b0101)),
                         (F(3, 16), 0, F(1, 4), F(7, 16)))

    def test_all_coverage_pairs_against_independent_closed_form(self):
        # Expected values use counts and intersections, not the operator under test.
        for red_mask, blue_mask, ar, ab in product(range(16), range(16), (F(1, 2), F(1)), (F(1, 2), F(1))):
            with self.subTest(red=red_mask, blue=blue_mask, ar=ar, ab=ab):
                prefix = paint((ZERO,) * 4, (ar, 0, 0, ar), red_mask)
                actual = resolve(paint(prefix, (0, 0, ab, ab), blue_mask))
                red = ar * (red_mask.bit_count() - ab * (red_mask & blue_mask).bit_count()) / 4
                blue = ab * blue_mask.bit_count() / 4
                self.assertEqual(actual, (red, 0, blue, red + blue))

    def test_early_resolve_error_matches_coverage_covariance(self):
        ar, ab = F(1, 2), F(3, 4)
        for a, b in product(range(16), repeat=2):
            prefix = paint((ZERO,) * 4, (ar, 0, 0, ar), a)
            direct = resolve(paint(prefix, (0, 0, ab, ab), b))
            early = resolve(paint((resolve(prefix),) * 4, (0, 0, ab, ab), b))
            expected_error = ar * ab * (F(a.bit_count() * b.bit_count(), 16) - F((a & b).bit_count(), 4))
            self.assertEqual(direct[0] - early[0], expected_error)
            self.assertEqual(direct[3] - early[3], expected_error)

    def test_sample_permutation_is_not_exact_restoration(self):
        prefix = paint((ZERO,) * 4, (1, 0, 0, 1), 0b0011)
        wrong = prefix[2:] + prefix[:2]
        self.assertEqual(resolve(prefix), resolve(wrong))
        self.assertNotEqual(resolve(paint(prefix, (0, 0, 1, 1), 0b0011)),
                            resolve(paint(wrong, (0, 0, 1, 1), 0b0011)))

    def test_repeated_translucent_suffix_requires_restoring_prefix(self):
        prefix = paint((ZERO,) * 4, (F(1, 2), 0, 0, F(1, 2)), 15)
        blue = (0, 0, F(1, 2), F(1, 2))
        first = paint(prefix, blue, 15)
        retry = paint(prefix, blue, 15)
        accumulated = paint(first, blue, 15)
        self.assertEqual(resolve(first), (F(1, 4), 0, F(1, 2), F(3, 4)))
        self.assertEqual(first, retry)
        self.assertEqual(resolve(accumulated), (F(1, 8), 0, F(3, 4), F(7, 8)))
        self.assertNotEqual(first, accumulated)

    def test_exact_associativity_does_not_prove_quantized_associativity(self):
        source = (F(1, 255),) * 4
        backdrop = (F(64, 255),) * 4
        self.assertEqual(over(source, over(source, backdrop)),
                         over(over(source, source), backdrop))
        direct = unorm8(over(source, unorm8(over(source, backdrop))))
        flattened = unorm8(over(unorm8(over(source, source)), backdrop))
        # First ordinary store: 65; second: 66. Isolated pair: 2; over 64: 65.
        # No intermediate below is a half-integer, so tie policy is irrelevant.
        self.assertEqual(direct, (F(66, 255),) * 4)
        self.assertEqual(flattened, (F(65, 255),) * 4)
        for value in (over(source, backdrop)[0], over(source, source)[0],
                      over(source, (F(65, 255),) * 4)[0],
                      over((F(2, 255),) * 4, backdrop)[0]):
            self.assertNotEqual((value * 255) % 1, F(1, 2))

    def test_quantized_prefix_includes_backdrop_and_preserves_store_sequence(self):
        source = (F(1, 255),) * 4
        backdrop = (F(64, 255),) * 4
        prefix = paint((backdrop,) * 4, source, 15, quantized=True)
        restored = tuple(tuple(channel for channel in pixel) for pixel in prefix)
        result = paint(restored, source, 15, quantized=True)
        self.assertEqual(resolve(result), (F(66, 255),) * 4)
        # This only demonstrates an ideal exact restore. A GPU restore must prove it.
        self.assertEqual(restored, prefix)

    def test_unorm8_code_roundtrip_is_exact_in_reference_only(self):
        for code in range(256):
            pixel = (F(code, 255),) * 4
            self.assertEqual(unorm8(pixel), pixel)

    def test_double_premultiplication_is_detected(self):
        source = premultiply((1, 0, 0, F(1, 2)))
        correct = over(source, (0, 0, 1, 1))
        wrong = over(premultiply(source), (0, 0, 1, 1))
        self.assertEqual(correct, (F(1, 2), 0, F(1, 2), 1))
        self.assertNotEqual(correct, wrong)

    def test_invalid_reference_inputs_fail_closed(self):
        for value in (True, 0.5, float('nan'), float('inf'), '1', -1, F(2)):
            with self.subTest(value=value), self.assertRaises(ValueError):
                unit(value)
        for pixel in ((1, 0, 0), (1, 0, 0, 0)):
            with self.assertRaises(ValueError):
                rgba(pixel)
        for mask in (-1, 16, True, F(1)):
            with self.assertRaises(ValueError):
                paint((ZERO,) * 4, ZERO, mask)
        with self.assertRaises(ValueError):
            resolve((ZERO,) * 3)

    def test_review_fixture_has_exact_original_bytes_and_parses(self):
        path = Path(__file__).with_name('fixtures') / 'showcase_triangle_light_vector_ending.py'
        source = path.read_bytes()
        self.assertEqual(len(source), 29435)
        self.assertEqual(hashlib.sha256(source).hexdigest(),
                         '32f2c6ee607bf9670ed47c77dd1f321ace576b5f725a523069bdcad54026bd2e')
        ast.parse(source, filename=str(path))  # No import, no mocked engine, no GPU claim.

    def test_reference_does_not_import_product_or_existing_compositor(self):
        tree = ast.parse(Path(__file__).read_text())
        allowed = {'fractions', 'ast', 'hashlib', 'itertools', 'pathlib', 'unittest'}
        for node in ast.walk(tree):
            if isinstance(node, ast.Import):
                self.assertTrue(all(alias.name in allowed for alias in node.names))
            elif isinstance(node, ast.ImportFrom):
                self.assertIn(node.module, allowed)


if __name__ == '__main__':
    unittest.main()
