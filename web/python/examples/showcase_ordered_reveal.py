"""Compare accumulating members with displaying one member at a time."""
from noon import *


class OrderedReveal(Scene):
    def construct(self):
        title = Text("Build a collection, or step through it", font_size=32).shift(3 * UP)
        top_label = Text("Accumulate", font_size=24, color=BLUE).move_to((-4.5, 1, 0))
        bottom_label = Text("One at a time", font_size=24, color=PINK).move_to((-4.5, -1, 0))
        top = VGroup(*[Circle(radius=0.35, color=BLUE).set_fill(BLUE, opacity=0.9).move_to((x, 1, 0)) for x in (-1.5, 0, 1.5, 3)])
        bottom = VGroup(*[Circle(radius=0.35, color=PINK).set_fill(PINK, opacity=0.9).move_to((x, -1, 0)) for x in (-1.5, 0, 1.5, 3)])
        caption = Text("Discrete visibility is the feature; the surrounding transitions stay smooth.", font_size=18).shift(2.7 * DOWN)
        self.play(FadeIn(title), FadeIn(top_label), FadeIn(bottom_label), FadeIn(caption), run_time=0.8)
        self.play(ShowIncreasingSubsets(top), run_time=3.0, rate_func=linear)
        self.wait(0.5)
        self.play(ShowSubmobjectsOneByOne(bottom), run_time=3.0, rate_func=linear)
        self.wait(1.2)
