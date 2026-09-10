from manim import *


class OrdinaryPathConstruction(Scene):
    def construct(self):
        point = lambda x, y: RIGHT * x + UP * y
        curves = VMobject(color="#58c4dd")
        curves.start_new_path(point(-3, -1)).add_line_to(point(-2, 0))
        curves.add_quadratic_bezier_curve_to(point(-1, 2), point(0, 0))
        curves.start_new_path(point(1, -1))
        curves.add_cubic_bezier_curve_to(point(1, 2), point(3, 2), point(3, -1))
        curves.close_path()
        polygon = Square(side_length=1).set_fill(YELLOW, opacity=0.3).shift(LEFT * 2 + DOWN * 2)
        self.add(curves, polygon)
        polygon.add_line_to(point(0, -2)).close_path()
        self.wait(0.2)
