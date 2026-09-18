"""Animated number-line walkthrough using only the supported shared API."""
from noon import *


class AnimatedNumberLine(Scene):
    def construct(self):
        title = Text("Number line: position and direction", font_size=28).shift(3 * UP)

        baseline = Line(5 * LEFT, 5 * RIGHT, color=WHITE)
        ticks = []
        labels = []
        for value in range(-4, 5):
            x = value * 1.1
            tick = Line(
                x * RIGHT + 0.12 * DOWN,
                x * RIGHT + 0.12 * UP,
                color=GRAY,
            )
            label = Text(str(value), font_size=18).move_to(x * RIGHT + 0.42 * DOWN)
            ticks.append(tick)
            labels.append(label)

        marker = Dot((-4 * 1.1) * RIGHT, color=YELLOW, radius=0.11)
        caption = Text("Move right: values increase", font_size=22).shift(2 * UP)

        self.play(Write(title), run_time=0.5)
        self.play(
            Create(baseline),
            *[Create(tick) for tick in ticks],
            *[FadeIn(label) for label in labels],
            run_time=1.0,
        )
        self.play(FadeIn(marker), Write(caption), run_time=0.5)
        self.play(marker.animate.move_to(0 * RIGHT), run_time=1.5)
        self.wait(0.5)
        self.play(marker.animate.move_to((3 * 1.1) * RIGHT), run_time=1.2)
        self.wait(0.5)
        self.play(marker.animate.move_to((-2 * 1.1) * RIGHT), run_time=1.5)
        self.wait(0.8)
