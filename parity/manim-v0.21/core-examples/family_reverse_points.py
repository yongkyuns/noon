from manim import *


class OrdinaryFamilyReversePoints(Scene):
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
        nested = Group(second)
        family = Group(first, nested)
        first_start, first_end = tuple(first.get_start()), tuple(first.get_end())
        second_start, second_end = tuple(second.get_start()), tuple(second.get_end())
        self.add(family)
        returned = family.reverse_points()
        assert returned is family
        assert all(abs(left - right) < 1e-8 for left, right in zip(first.get_start(), first_end))
        assert all(abs(left - right) < 1e-8 for left, right in zip(first.get_end(), first_start))
        assert all(abs(left - right) < 1e-8 for left, right in zip(second.get_start(), second_end))
        assert all(abs(left - right) < 1e-8 for left, right in zip(second.get_end(), second_start))
        self.wait(0.2)
