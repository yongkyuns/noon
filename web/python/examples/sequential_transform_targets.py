"""Sequential full-state targets capture each preceding effective endpoint."""
from noon import Circle, Scene, Succession, Transform, WHITE, linear


class SequentialTransformTargets(Scene):
    def construct(self):
        circle = Circle(radius=0.4).set_fill(WHITE, opacity=1)
        first = circle.copy().move_to((2, 1, 0))
        second = circle.copy().move_to((4, 0, 0))
        self.add(circle)
        self.play(Succession(
            Transform(circle, first, run_time=1, rate_func=linear),
            Transform(circle, second, run_time=1, rate_func=linear),
        ), rate_func=linear)
        assert circle.get_center() == (4.0, 0.0)
