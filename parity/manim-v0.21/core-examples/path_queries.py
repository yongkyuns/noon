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
                self.add(Dot(shape.point_from_proportion(alpha), color="#ffcc44"))
        before = rectangle.get_start()
        self.wait(0.2)
        after = rectangle.get_start()
        assert abs(before[0] - after[0]) < 1e-6
        assert abs(before[1] - after[1]) < 1e-6
