from manim import *


class NumberPlaneGrid(Scene):
    def construct(self):
        # Keep the retained-grid oracle deterministic at the pinned 960×540
        # sample resolution: transformed line endpoints, major strokes, and
        # faded strokes all land on integral pixel coverage. The semantic
        # default-style cases remain covered by the coordinate fixtures.
        major_width = 80 / 27
        faded_width = 40 / 27
        major = NumberPlane(
            x_range=[-3, 3, 1], y_range=[-2, 2, 1],
            faded_line_ratio=0,
            axis_config={"stroke_width": major_width},
            background_line_style={"stroke_width": major_width},
        ).scale(0.8).shift(3.2 * LEFT)
        subdivided = NumberPlane(
            x_range=[2, 6, 1], y_range=[-3, 1, 0.75],
            x_length=16 / 3, y_length=160 / 27,
            faded_line_ratio=3,
            axis_config={"stroke_width": major_width},
            background_line_style={"stroke_color": GREEN, "stroke_width": major_width},
            faded_line_style={"stroke_color": BLUE, "stroke_width": faded_width, "stroke_opacity": 0.5},
        ).scale(0.8).shift(3.2 * RIGHT)
        self.add(major, subdivided)
        self.wait(0.2)
