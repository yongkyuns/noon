from manim import *


class NumberPlaneGrid(Scene):
    def construct(self):
        major = NumberPlane(
            x_range=[-3, 3, 1], y_range=[-2, 2, 1],
            faded_line_ratio=0,
        ).scale(0.8).shift(3.2 * LEFT)
        subdivided = NumberPlane(
            x_range=[2, 6, 1], y_range=[-3, 1, 0.75], x_length=5, y_length=3,
            faded_line_ratio=3,
            background_line_style={"stroke_color": GREEN, "stroke_width": 2},
            faded_line_style={"stroke_color": BLUE, "stroke_width": 1, "stroke_opacity": 0.5},
        ).scale(0.8).shift(3.2 * RIGHT)
        self.add(major, subdivided)
        self.wait(0.2)
