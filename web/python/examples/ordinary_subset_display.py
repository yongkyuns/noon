from noon import *


class OrdinarySubsetDisplay(Scene):
    def construct(self):
        increasing = VGroup(
            Circle(radius=0.3).set_fill(RED, opacity=1).shift(LEFT + 0.7 * UP),
            Circle(radius=0.3).set_fill(GREEN, opacity=1).shift(0.7 * UP),
            Circle(radius=0.3).set_fill(BLUE, opacity=1).shift(RIGHT + 0.7 * UP),
        )
        one_by_one = VGroup(
            Circle(radius=0.3).set_fill(ORANGE, opacity=1).shift(LEFT + 0.7 * DOWN),
            Circle(radius=0.3).set_fill(PINK, opacity=1).shift(0.7 * DOWN),
            Circle(radius=0.3).set_fill("#F7D96F", opacity=1).shift(RIGHT + 0.7 * DOWN),
        )
        self.play(ShowIncreasingSubsets(increasing, run_time=3, rate_func=linear))
        self.play(ShowSubmobjectsOneByOne(one_by_one, run_time=3, rate_func=linear))
        self.wait(0.25)
