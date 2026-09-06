"""Ordinary synchronous continuation over the shared canonical affine runtime."""

from noon import Circle, Color, Scene, Transform, linear


class OrdinaryAffineSynchronousContinuation(Scene):
    def construct(self):
        circle = Circle(radius=0.4).set_fill(Color(0.0, 0.4, 1.0), opacity=1.0)
        self.add(circle)

        first_target = circle.copy().shift((2.0, -1.0, 0.0))
        self.play(Transform(circle, first_target), run_time=2.0, rate_func=linear)
        assert self.time == 2.0
        assert circle.get_center() == (2.0, -1.0)

        self.wait(1.0)
        assert self.time == 3.0
        assert circle.get_center() == (2.0, -1.0)

        circle.shift((1.0, 0.0, 0.0))
        assert circle.get_center() == (3.0, -1.0)
        second_target = circle.copy().shift((2.0, 0.0, 0.0))
        self.play(Transform(circle, second_target), run_time=1.0, rate_func=linear)
        assert self.time == 4.0
        assert circle.get_center() == (5.0, -1.0)
