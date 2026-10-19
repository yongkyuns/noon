from manim import *


class OrdinaryFamilySortOrder(Scene):
    def construct(self):
        left = Square(side_length=2, color="#FF0000", fill_opacity=1, stroke_width=0).shift(LEFT * 0.5)
        right = Square(side_length=2, color="#0000FF", fill_opacity=1, stroke_width=0).shift(RIGHT * 0.5)
        family = VGroup(left, right)
        self.add(family)
        returned = family.sort(point_to_num_func=lambda point: -point[0])
        assert returned is family
        assert list(family) == [right, left]
        self.wait(0.2)
