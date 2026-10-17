from manim import *


class OrdinaryPathQueries(Scene):
    def construct(self):
        rectangle = Rectangle(width=2, height=1, color=WHITE)
        rectangle.stretch_to_fit_width(4).rotate(0.25).shift(LEFT * 2.5)
        ellipse = Circle(radius=0.8, color=WHITE)
        ellipse.stretch_to_fit_width(2.4).shift(RIGHT * 2.5)
        for shape in (rectangle, ellipse):
            self.add(shape)
            assert shape.get_arc_length() > 0
            for alpha in (0.125, 0.375, 0.625, 0.875):
                point = shape.point_from_proportion(alpha)
                assert abs(shape.proportion_from_point(point) - alpha) < 1e-5
                self.add(Dot(point, color="#ffcc44"))
        try:
            rectangle.proportion_from_point(ORIGIN)
        except ValueError:
            pass
        else:
            raise AssertionError("an off-path point must be rejected")
        loop = CubicBezier(ORIGIN, (1, 1, 0), (-1, 1, 0), ORIGIN)
        assert abs(loop.proportion_from_point(ORIGIN) - 1.0) < 1e-6
        target = rectangle.copy().stretch(1.2, 0)
        self.play(Transform(rectangle, target), run_time=1, rate_func=linear)
        self.wait(0.2)
        self.remove(ellipse)
        self.play(Create(ellipse), run_time=1, rate_func=linear)
