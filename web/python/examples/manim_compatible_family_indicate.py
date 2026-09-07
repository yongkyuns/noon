from noon import *


class FamilyTransformIndicateDemo(Scene):
    def construct(self):
        left = (
            Square(side_length=0.9)
            .set_fill(PINK, opacity=0.9)
            .set_stroke(opacity=0.0)
            .shift(LEFT * 2)
        )
        right = (
            Circle(radius=0.45)
            .set_fill(BLUE, opacity=0.9)
            .set_stroke(opacity=0.0)
        )
        family = VGroup(left, right)

        self.add(family)
        self.play(
            family.animate(run_time=2.0, rate_func=linear, lag_ratio=0.5).shift(RIGHT)
        )
        self.play(Indicate(family, run_time=2.0, lag_ratio=0.5))
        self.wait(0.25)
        left.shift(RIGHT)
        self.wait(0.25)
