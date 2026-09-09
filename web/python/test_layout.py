import unittest

from noon import (
    BLUE,
    DEGREES,
    DEFAULT_MOBJECT_TO_EDGE_BUFFER,
    DEFAULT_MOBJECT_TO_MOBJECT_BUFFER,
    LEFT,
    PI,
    RED,
    RIGHT,
    UP,
    UR,
    Circle,
    Scene,
    Square,
    VGroup,
    Vec2,
    color_from_hex,
)


class PublicAuthoringTests(unittest.TestCase):
    def test_direction_arithmetic_stays_tuple_compatible(self) -> None:
        point = 2.0 * LEFT + 0.75 * UP
        self.assertIsInstance(point, tuple)
        self.assertEqual(point, (-2.0, 0.75))
        self.assertAlmostEqual(90 * DEGREES, PI / 2.0)

    def test_named_palette_matches_canonical_values(self) -> None:
        self.assertEqual(BLUE, color_from_hex("#58C4DD"))
        self.assertEqual(RED, color_from_hex(0xFC6255))


if __name__ == "__main__":
    unittest.main()
