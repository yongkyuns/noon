"""Shared Create/Uncreate reveal timing for plain Text and Text families."""

from noon import Create, DOWN, LEFT, RIGHT, Scene, Square, Text, Uncreate, UP, VGroup, linear


class OrdinaryTextFamilyReveal(Scene):
    def construct(self):
        left = Text("I").shift(3 * LEFT + 0.75 * UP)
        right = Text("LONG").shift(0.75 * UP)
        family = VGroup(left, right)
        solo = Text("ONE").shift(2 * LEFT + 1.25 * DOWN)
        moving = Square(0.6).shift(RIGHT + 1.25 * DOWN)
        self.add(moving)

        try:
            self.play(
                Create(family, run_time=0.25, lag_ratio=0.25),
                left.animate.shift(UP),
            )
            raise AssertionError("overlapping family Create must fail")
        except ValueError:
            pass
        assert self.mobjects == [moving]
        assert abs(left.get_center().y - 0.75) < 1e-6

        self.play(
            Create(family, run_time=2.0, lag_ratio=0.25, rate_func=linear),
            Create(solo, run_time=2.0, lag_ratio=0.25, rate_func=linear),
            moving.animate.shift(2 * RIGHT),
            run_time=2.0,
            rate_func=linear,
        )
        assert self.mobjects == [moving, family, solo]
        assert abs(moving.get_center().x - 3.0) < 1e-6

        self.play(
            Uncreate(family, run_time=1.0, lag_ratio=0.25, rate_func=linear),
            Uncreate(solo, run_time=1.0, lag_ratio=0.25, rate_func=linear),
            run_time=1.0,
            rate_func=linear,
        )
        assert self.mobjects == [moving]
        self.wait(0.25)
