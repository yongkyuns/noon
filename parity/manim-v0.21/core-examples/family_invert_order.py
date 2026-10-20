from manim import *


class OrdinaryFamilyInvertOrder(Scene):
    def construct(self):
        first = Square(side_length=2, color="#FF0000", fill_opacity=1, stroke_width=0)
        second = Square(side_length=2, color="#0000FF", fill_opacity=1, stroke_width=0).shift(RIGHT)
        family = VGroup(first, second)
        family.invert()
        assert list(family) == [second, first]
        self.add(family)
        self.wait(0.2)
