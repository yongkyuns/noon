from manim import *


class UnequalFamilyTransformExpansion(Scene):
    def construct(self):
        source = VGroup(
            Circle(
                radius=0.65,
                fill_color=BLUE,
                fill_opacity=1.0,
                stroke_opacity=0.0,
            ).move_to(2.5 * LEFT),
            Circle(
                radius=0.65,
                fill_color=BLUE,
                fill_opacity=1.0,
                stroke_opacity=0.0,
            ).move_to(2.5 * RIGHT),
        )
        target = VGroup(
            Circle(
                radius=0.65,
                fill_color=BLUE,
                fill_opacity=1.0,
                stroke_opacity=0.0,
            ).move_to(3.0 * LEFT),
            Circle(
                radius=0.65,
                fill_color=BLUE,
                fill_opacity=1.0,
                stroke_opacity=0.0,
            ),
            Circle(
                radius=0.65,
                fill_color=BLUE,
                fill_opacity=1.0,
                stroke_opacity=0.0,
            ).move_to(3.0 * RIGHT),
        )
        self.add(source)
        self.play(Transform(source, target), run_time=1.0, rate_func=linear)


class UnequalFamilyTransformContraction(Scene):
    def construct(self):
        source = VGroup(
            Circle(
                radius=0.65,
                fill_color=GREEN,
                fill_opacity=1.0,
                stroke_opacity=0.0,
            ).move_to(3.0 * LEFT),
            Circle(
                radius=0.65,
                fill_color=GREEN,
                fill_opacity=1.0,
                stroke_opacity=0.0,
            ),
            Circle(
                radius=0.65,
                fill_color=GREEN,
                fill_opacity=1.0,
                stroke_opacity=0.0,
            ).move_to(3.0 * RIGHT),
        )
        target = VGroup(
            Circle(
                radius=0.65,
                fill_color=GREEN,
                fill_opacity=1.0,
                stroke_opacity=0.0,
            ).move_to(2.5 * LEFT),
            Circle(
                radius=0.65,
                fill_color=GREEN,
                fill_opacity=1.0,
                stroke_opacity=0.0,
            ).move_to(2.5 * RIGHT),
        )
        self.add(source)
        self.play(Transform(source, target), run_time=1.0, rate_func=linear)
