from manim import *


class UnequalFamilyPathTransformExpansion(Scene):
    def construct(self):
        source = VGroup(
            Arc(
                radius=0.65,
                start_angle=0.25,
                angle=1.45,
                color=RED,
                stroke_width=8.0,
            ).move_to([-2.0, 0, 0]),
            Arc(
                radius=0.55,
                start_angle=0.25,
                angle=2.05,
                color=BLUE,
                stroke_width=8.0,
            ).move_to([2.0, 0, 0]),
        )
        target = VGroup(
            Arc(
                radius=0.50,
                start_angle=0.25,
                angle=2.20,
                color=GREEN,
                stroke_width=8.0,
            ).move_to([-3.0, 0, 0]),
            Arc(
                radius=0.75,
                start_angle=0.25,
                angle=1.70,
                color=YELLOW,
                stroke_width=8.0,
            ).move_to([0.0, 0, 0]),
            Arc(
                radius=0.60,
                start_angle=0.25,
                angle=2.55,
                color=PURPLE,
                stroke_width=8.0,
            ).move_to([3.0, 0, 0]),
        )
        self.add(source)
        self.play(Transform(source, target, run_time=1.0, rate_func=linear))
