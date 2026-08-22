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

    def test_next_to_uses_object_bounds_and_default_buffer(self) -> None:
        circle = Circle(1.0)
        square = Square(1.0).next_to(circle, RIGHT)

        circle_right = circle.get_center().x + circle.width / 2.0
        square_left = square.get_center().x - square.width / 2.0
        self.assertAlmostEqual(
            square_left - circle_right,
            DEFAULT_MOBJECT_TO_MOBJECT_BUFFER,
        )

    def test_vgroup_arrange_uses_bounds_not_fixed_slots(self) -> None:
        small = Circle(0.25)
        large = Circle(0.75)
        square = Square(0.5)
        group = VGroup(small, large, square).arrange(RIGHT, buff=0.4)

        self.assertEqual(len(group), 3)
        self.assertAlmostEqual(
            large.get_center().x - large.width / 2.0 - (small.get_center().x + small.width / 2.0),
            0.4,
        )
        self.assertAlmostEqual(
            square.get_center().x - square.width / 2.0 - (large.get_center().x + large.width / 2.0),
            0.4,
        )
        self.assertAlmostEqual(group.get_center().x, 0.0)

    def test_to_corner_uses_shared_logical_frame(self) -> None:
        square = Square(1.0).to_corner(UR)
        self.assertAlmostEqual(square.get_center().y + 0.5, 4.0 - DEFAULT_MOBJECT_TO_EDGE_BUFFER)

    def test_scene_cursor_and_animate_lower_to_existing_transform_track(self) -> None:
        scene = Scene()
        circle = Circle(0.5, color=BLUE)
        scene.add(circle, key="circle")

        scene.play(circle.animate.shift(RIGHT), run_time=1.25)
        scene.wait(0.5)

        track = scene.to_document()["tracks"][0]
        self.assertEqual(track["property"], "transform")
        self.assertEqual(track["timing"]["start_time"], 0.0)
        self.assertEqual(track["timing"]["duration"], 1.25)
        self.assertEqual(scene.time, 1.75)
        self.assertEqual(
            track["values"]["object"]["to"]["transform"]["translation"],
            {"x": 1.0, "y": 0.0},
        )

    def test_low_level_position_api_remains_an_escape_hatch(self) -> None:
        scene = Scene()
        circle = scene.circle(0.5)
        scene.animate_position(circle, Vec2(0.0, 0.0), Vec2(1.0, 2.0), duration=1.0)
        self.assertEqual(scene.to_document()["tracks"][0]["property"], "position")


if __name__ == "__main__":
    unittest.main()
