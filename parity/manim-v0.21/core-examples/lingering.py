from manim import *


class LingeringRate(Scene):
    def construct(self):
        moving = Square(side_length=0.65, color=PURE_RED, fill_opacity=1, stroke_width=0)
        moving.shift(2 * LEFT + 2 * UP)
        reversing = Square(side_length=0.65, color=PURE_GREEN, fill_opacity=1, stroke_width=0)
        reversing.shift(2 * LEFT + UP)
        self.add(moving)
        self.play(AnimationGroup(
            moving.animate(run_time=1, rate_func=lingering).shift(4 * RIGHT),
            Create(reversing, run_time=1, rate_func=lingering, reverse_rate_function=True),
            rate_func=linear,
        ))
