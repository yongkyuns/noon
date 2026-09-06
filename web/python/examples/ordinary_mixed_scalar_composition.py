from noon import *


class OrdinaryMixedScalarComposition(Scene):
    def construct(self):
        circle = Circle(radius=0.3).set_fill(BLUE, opacity=0.7)
        square = Square(side_length=1).set_fill(PINK, opacity=0.7).shift(DOWN)
        self.add(circle, square)
        tracker = self.value_tracker(0)
        self.bind_position(circle, tracker, direction=RIGHT, offset=LEFT * 2 + UP)
        self.play(Succession(
            Wait(0.5),
            AnimationGroup(
                tracker.animate(run_time=2, rate_func=linear).set_value(4),
                Rotate(square, PI, run_time=2, rate_func=linear),
                rate_func=smooth,
            ),
            rate_func=linear,
        ))
        assert abs(tracker.get_value() - 4) < 1e-6
        assert abs(circle.get_center().x - 2) < 1e-6
        self.wait(0.25)
        tracker.set_value(3)
        self.wait(0.25)
        assert abs(circle.get_center().x - 1) < 1e-6
