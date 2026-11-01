"""Paired with Rust example_scenes::sampled_motion::session()."""

from noon import *
from math import pi


class SampledMotionExample(Scene):
    def construct(self):
        line = Line((-0.5, 0, 0), (0.5, 0, 0), color=BLUE)
        line.rotate(0.25).move_to((-2, 1, 0))
        square = Square(side_length=0.6, color=RED, fill_opacity=1)
        square.rotate(-0.4).move_to((-2, -1, 0))
        self.add(line, square)
        keys = [0.13, 0.4, 0.73, 1.0]
        self.play(
            SampledMotion(
                line,
                keys,
                [(-1, 2, 0), (1, 1, 0), (2, 2, 0), (2, 0, 0)],
                angles=[-6 * pi * phase for phase in keys],
                run_time=2.3,
                rate_func=linear,
            ),
            SampledMotion(
                square,
                keys,
                [(-1, -2, 0), (1, -1, 0), (2, -2, 0), (2, 0, 0)],
                angles=[8 * pi * phase for phase in keys],
                run_time=2.3,
                rate_func=linear,
            ),
            run_time=2.3,
            rate_func=linear,
        )
