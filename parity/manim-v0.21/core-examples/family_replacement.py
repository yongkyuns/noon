from manim import *


class FamilyReplacement(Scene):
    def construct(self):
        first = Rectangle(width=2, height=1).set_fill("#4488FF", opacity=1).set_stroke(width=0)
        second = Square(side_length=1).set_fill("#FFCC44", opacity=1).set_stroke(width=0)
        target = Rectangle(width=4, height=2).set_fill("#FF4466", opacity=1).set_stroke(width=0)
        first.shift(2 * LEFT)
        second.shift(2 * RIGHT)
        target.shift(2 * DOWN)
        family = VGroup(first, VGroup(first, second))
        family.replace(target)
        self.add(family, target)
        family.replace(target, stretch=True)
        family.shift(4 * UP)
        assert abs(family.width - target.width) < 1e-6
        assert abs(family.height - target.height) < 1e-6
        self.wait(0.2)
