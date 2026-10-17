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
            pieces = shape.get_nth_curve_length_pieces(0)
            assert len(pieces) == 9
            assert abs(shape.get_nth_curve_length(0) - sum(pieces)) < 1e-10
            custom_pieces = shape.get_nth_curve_length_pieces(0, sample_points=4)
            assert len(custom_pieces) == 3
            assert abs(shape.get_nth_curve_length(0, sample_points=4) - sum(custom_pieces)) < 1e-10
            assert len(shape.get_nth_curve_length_pieces(0, sample_points=1)) == 0
            assert shape.get_nth_curve_length(0, sample_points=1) == 0
            for alpha in (0.125, 0.375, 0.625, 0.875):
                self.add(Dot(shape.point_from_proportion(alpha), color="#ffcc44"))
        target = rectangle.copy().stretch(1.2, 0)
        self.play(Transform(rectangle, target), run_time=1, rate_func=linear)
        self.wait(0.2)
        self.remove(ellipse)
        self.play(Create(ellipse), run_time=1, rate_func=linear)
