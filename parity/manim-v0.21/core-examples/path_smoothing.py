from manim import *

class OrdinaryPathSmoothing(Scene):
    def construct(self):
        point = lambda x, y: RIGHT * x + UP * y
        open_path = VMobject().set_points_as_corners([point(-3, 1), point(-2, 2), point(-1, 1)])
        closed_path = Square(side_length=1.5).shift(point(2, 1.5))
        family = VGroup(open_path, closed_path).set_color(BLUE)
        smoothed = family.copy().make_smooth().shift(DOWN * 3).set_color(YELLOW)
        self.add(family, smoothed)
        self.wait(0.2)
