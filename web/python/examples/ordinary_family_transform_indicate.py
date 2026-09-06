from noon import *


class OrdinaryFamilyTransformIndicate(Scene):
    def construct(self):
        left = (
            Square(side_length=0.6)
            .set_fill(PINK, opacity=0.9)
            .set_stroke(opacity=0)
            .shift(LEFT * 2)
        )
        right = (
            Circle(radius=0.3)
            .set_fill(BLUE, opacity=0.9)
            .set_stroke(opacity=0)
        )
        family = VGroup(left, right)
        self.add(family)
        self.play(
            family.animate(run_time=2, rate_func=linear, lag_ratio=0.5).shift(RIGHT)
        )
        assert abs(left.get_center().x + 1) < 1e-6
        assert abs(right.get_center().x - 1) < 1e-6
        self.play(Indicate(family, run_time=2, lag_ratio=0.5))
        assert abs(left.get_center().x + 1) < 1e-6
        assert abs(right.get_center().x - 1) < 1e-6
        self.wait(0.25)
        left.shift(RIGHT)
        self.wait(0.25)
        assert abs(left.get_center().x) < 1e-6
