"""Shared Circle.surround sizing from Rust circle_surround, including live edits."""
from noon import *


class CircleSurround(Scene):
    def construct(self):
        left_target = Rectangle(width=2, height=1).set_fill("#4488FF", opacity=0.4).set_stroke(width=0)
        right_target = Rectangle(width=2, height=1).set_fill("#FF4466", opacity=0.4).set_stroke(width=0)
        left_target.shift(3 * LEFT)
        right_target.shift(3 * RIGHT)
        left_circle = Circle(radius=0.4).set_stroke("#58C4DD", width=3)
        right_circle = Circle(radius=0.4).set_stroke("#FFCC44", width=3)
        left_circle.surround(left_target)
        self.add(left_target, left_circle, right_target, right_circle)
        self.wait(0.1)
        assert abs(right_circle.width - 0.8) < 1e-6
        right_circle.surround(right_target, dim_to_match=1, stretch=True, buffer_factor=0.9)
        assert abs(left_circle.width - 5 ** 0.5 * 1.2) < 1e-6
        assert abs(right_circle.height / right_circle.width - 0.5) < 1e-6
        self.wait(0.1)
