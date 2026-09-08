from noon import *


class ReturningTransform(Scene):
    def construct(self):
        circle = Circle(radius=0.4, color=WHITE, fill_opacity=1)
        first = circle.copy().move_to((2, 1, 0))
        second = circle.copy().move_to((4, 3, 0))
        self.add(circle)
        self.play(Succession(
            Transform(circle, first, run_time=1, rate_func=linear),
            Transform(circle, second, run_time=1, rate_func=there_and_back),
        ), rate_func=linear)
        assert abs(circle.get_x() - 2) < 1e-6
        assert abs(circle.get_y() - 1) < 1e-6
