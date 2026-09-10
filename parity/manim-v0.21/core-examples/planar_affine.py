"""Paired with the ordinary Rust native/direct-WASM planar_affine example."""
from manim import *


class OrdinaryPlanarAffine(Scene):
    def construct(self):
        rectangle = Rectangle(width=1.5, height=.7).set_fill("#FF4466", opacity=1).set_stroke(width=0)
        rectangle.rotate(.3).shift(2 * LEFT + UP)
        line = Line(3 * LEFT + DOWN, LEFT).set_stroke("#4488FF")
        reflected_rectangle = rectangle.copy().set_fill("#FFCC44", opacity=1)
        reflected_line = line.copy().set_stroke("#44FF88")
        reflected = VGroup(reflected_rectangle, VGroup(reflected_rectangle, reflected_line))
        reflected.flip(UP, about_point=ORIGIN)
        self.add(rectangle, line, reflected)
        reflected.rotate(PI / 6, about_point=2 * RIGHT)
        reflected.flip(UP + RIGHT)
        self.wait(.2)
