from noon import *


class OrdinaryPathConstruction(Scene):
    def construct(self):
        curves = VMobject(color="#58c4dd")
        curves.start_new_path((-3, -1, 0)).add_line_to((-2, 0, 0))
        curves.add_quadratic_bezier_curve_to((-1, 2, 0), (0, 0, 0))
        curves.start_new_path((1, -1, 0))
        curves.add_cubic_bezier_curve_to((1, 2, 0), (3, 2, 0), (3, -1, 0))
        curves.close_path()
        polygon = Square(side_length=1).set_fill(YELLOW, opacity=0.3).shift(LEFT * 2 + DOWN * 2)
        self.add(curves, polygon)
        polygon.add_line_to((0, -2, 0)).close_path()
        self.wait(0.2)
