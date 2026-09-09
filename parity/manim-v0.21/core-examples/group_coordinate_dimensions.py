from manim import *


class GroupCoordinateDimensions(Scene):
    def construct(self):
        first = Rectangle(width=2, height=1).set_fill("#4488FF", opacity=1).set_stroke(width=0)
        second = Square(side_length=1).set_fill("#FFCC44", opacity=1).set_stroke(width=0)
        target = Rectangle(width=0.5, height=2).set_fill("#FF4466", opacity=1).set_stroke(width=0)
        first.shift(2 * LEFT)
        second.shift(2 * RIGHT)
        target.shift(RIGHT + UP)
        family = VGroup(first, VGroup(second), first)
        family.width = 4
        family.height = 2
        family.set_x(3, RIGHT).set_y(-2, DOWN)
        family.match_x(target, LEFT).match_y(target, UP)
        self.add(family, target)
        self.wait(0.2)
