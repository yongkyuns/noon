from manim import *


class OrdinaryFamilyRepeatPoints(Scene):
    def construct(self):
        first = VMobject(color=RED, stroke_width=8)
        first.set_points_as_corners([LEFT * 3 + DOWN, LEFT * 2 + UP, LEFT + DOWN])
        first.start_new_path(DOWN).add_line_to(RIGHT + UP)
        second = VMobject(color=BLUE, stroke_width=8)
        second.set_points_as_corners([
            RIGHT * 2 + DOWN,
            RIGHT * 3 + UP,
            RIGHT * 4 + DOWN,
        ])
        family = Group(first, Group(second))
        self.add(family)
        returned = family.repeat(2)
        assert returned is family
        assert first.get_num_points() == 2 * 12
        assert second.get_num_points() == 2 * 8
        self.wait(0.2)
