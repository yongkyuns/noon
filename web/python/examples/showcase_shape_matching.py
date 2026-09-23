"""Match vector geometry even when the target group's member order changes."""
from noon import *


def triangle():
    return VMobject().set_points_as_corners([(-0.7, -0.6, 0), (0.7, -0.3, 0), (-0.2, 0.7, 0), (-0.7, -0.6, 0)])


def kite():
    return VMobject().set_points_as_corners([(0, -0.7, 0), (0.8, 0, 0), (0, 0.7, 0), (-0.5, 0, 0), (0, -0.7, 0)])


class MatchingByShape(Scene):
    def construct(self):
        title = Text("Match by shape, not list position", font_size=32).shift(2.8 * UP)
        before = Text("Source: triangle, kite", font_size=24).shift(2.5 * DOWN)
        after = Text("Target: kite, triangle", font_size=24).shift(2.5 * DOWN)
        source = VGroup(
            triangle().set_fill(PINK, opacity=0.9).set_stroke(opacity=0).shift(2.2 * LEFT + 0.8 * UP),
            kite().set_fill(BLUE, opacity=0.9).set_stroke(opacity=0).shift(2.2 * RIGHT + 0.8 * UP),
        )
        target = VGroup(
            kite().set_fill(BLUE, opacity=0.9).set_stroke(opacity=0).shift(2.2 * LEFT + 0.8 * DOWN),
            triangle().set_fill(PINK, opacity=0.9).set_stroke(opacity=0).shift(2.2 * RIGHT + 0.8 * DOWN),
        )
        self.play(FadeIn(title), FadeIn(before), run_time=0.7)
        self.play(FadeIn(source), run_time=1.0)
        self.wait(0.8)
        self.play(FadeOut(before), run_time=0.3)
        self.play(FadeIn(after), run_time=0.4)
        self.play(TransformMatchingShapes(source, target), run_time=2.4, rate_func=smooth)
        self.wait(0.5)
        self.play(Indicate(target), run_time=1.0)
        self.wait(1.2)
