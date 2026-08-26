# Source-equivalent ManimCE v0.21 geometry breadth example.
# Upstream constructor semantics: manim/mobject/geometry/arc.py (v0.21.0).

from noon import *


class DotEllipseParity(Scene):
    def construct(self):
        default_dot = Dot(point=3 * LEFT + 1.5 * UP)
        blue_dot = Dot(point=1.5 * LEFT + 1.5 * UP, radius=0.18, color=BLUE)
        wide = Ellipse(
            width=2.4,
            height=1.0,
            fill_color=GREEN,
            fill_opacity=1.0,
            stroke_opacity=0.0,
        ).move_to(1.0 * RIGHT + 1.5 * UP)
        tall = Ellipse(
            width=1.1,
            height=2.2,
            fill_color=PURPLE,
            fill_opacity=1.0,
            stroke_opacity=0.0,
        ).rotate(PI / 6).move_to(2.5 * RIGHT + 1.5 * DOWN)

        self.add(default_dot, blue_dot, wide, tall)
        self.play(tall.animate.shift(ORIGIN))


result = DotEllipseParity()
result.setup()
try:
    result.construct()
finally:
    result.tear_down()
