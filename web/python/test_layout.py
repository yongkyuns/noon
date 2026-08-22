import math
import unittest

from noon import Scene
from noon_layout import DOWN, LEFT, ORIGIN, RIGHT, UP, Vec2, arrange, grid, polar


class LayoutTests(unittest.TestCase):
    def test_direction_arithmetic_stays_tuple_compatible(self) -> None:
        point = 2.0 * LEFT + 0.75 * UP
        self.assertIsInstance(point, tuple)
        self.assertEqual(point, (-2.0, 0.75))

        scene = Scene()
        obj = scene.circle(0.5, position=point)
        translation = scene.to_document()["objects"][obj.id]["transform"]["translation"]
        self.assertEqual(translation, {"x": -2.0, "y": 0.75})

    def test_arrange_centers_positions_along_any_direction(self) -> None:
        self.assertEqual(
            arrange(3, direction=RIGHT, spacing=2.0),
            (Vec2(-2.0, 0.0), ORIGIN, Vec2(2.0, 0.0)),
        )
        diagonal = arrange(2, direction=RIGHT + UP, spacing=math.sqrt(2.0))
        self.assertAlmostEqual(diagonal[0].x, -0.5)
        self.assertAlmostEqual(diagonal[0].y, -0.5)
        self.assertAlmostEqual(diagonal[1].x, 0.5)
        self.assertAlmostEqual(diagonal[1].y, 0.5)

    def test_grid_is_row_major_and_centered(self) -> None:
        self.assertEqual(
            grid(2, 3, spacing=(2.0, 1.0)),
            (
                Vec2(-2.0, 0.5),
                Vec2(0.0, 0.5),
                Vec2(2.0, 0.5),
                Vec2(-2.0, -0.5),
                Vec2(0.0, -0.5),
                Vec2(2.0, -0.5),
            ),
        )

    def test_polar_positions_are_evenly_spaced(self) -> None:
        points = polar(4, radius=2.0, start_angle=0.0)
        expected = (RIGHT * 2.0, UP * 2.0, LEFT * 2.0, DOWN * 2.0)
        for point, target in zip(points, expected):
            self.assertAlmostEqual(point.x, target.x)
            self.assertAlmostEqual(point.y, target.y)


if __name__ == "__main__":
    unittest.main()
