"""Paired with Rust example_scenes::phase_reveal::session()."""

from noon import *


class PhaseRevealExample(Scene):
    def construct(self):
        keys = [0.0, 0.13, 0.4, 0.73, 1.0]
        points = [(-3, -1, 0), (-2, 1, 0), (0.5, -1, 0), (2, 1, 0), (2.5, -0.5, 0)]
        trace = VMobject(stroke_color=BLUE, stroke_width=6, fill_opacity=0)
        trace.set_points_as_corners(points)
        pen = Dot(points[0], radius=0.09, color=RED)
        self.add(pen)
        self.play(
            PhaseReveal(trace, keys, run_time=2.3, rate_func=linear),
            SampledMotion(pen, keys[1:], points[1:], run_time=2.3, rate_func=linear),
            run_time=2.3,
            rate_func=linear,
        )
