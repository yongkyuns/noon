from manim import *


class CubicBezierConstructor(Scene):
    def construct(self):
        curve = CubicBezier(
            (-3, -1, 0), (-1, 2, 0), (1, -2, 0), (3, 1, 0),
            stroke_color=BLUE, stroke_width=6, fill_opacity=0,
        )
        assert curve.get_num_points() == 4
        assert curve.get_num_curves() == 1
        assert abs(curve.point_from_proportion(0.5)[0]) < 1e-6
        assert abs(curve.point_from_proportion(0.5)[1]) < 1e-6
        self.add(curve)
        self.wait(0.2)
