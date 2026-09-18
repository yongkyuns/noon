"""Animate Noon's shared Rust-backed Manim-compatible NumberLine."""
from noon import *


class AnimatedNumberLine(Scene):
    def construct(self):
        title = Text("NumberLine: position and direction", font_size=28).shift(3 * UP)
        caption = Text("Move right: values increase", font_size=22).shift(2 * UP)

        number_line = NumberLine(
            [-4, 4, 1],
            length=8.8,
            include_ticks=True,
            color=WHITE,
        )
        number_line.add_numbers(
            [-4, -3, -2, -1, 0, 1, 2, 3, 4],
            font_size=18,
        )
        positions = {value: number_line.n2p(value) for value in (-4, -2, 0, 3)}
        marker = Dot(positions[-4], color=YELLOW, radius=0.11)

        self.play(Write(title), run_time=0.5)
        self.play(Create(number_line), run_time=1.0)
        self.play(FadeIn(marker), Write(caption), run_time=0.5)

        # Positions come from the authoritative NumberLine mapping rather than
        # duplicated Python coordinate math.
        self.play(marker.animate.move_to(positions[0]), run_time=1.5)
        self.wait(0.5)
        self.play(marker.animate.move_to(positions[3]), run_time=1.2)
        self.wait(0.5)

        self.play(marker.animate.move_to(positions[-2]), run_time=1.5)
        self.wait(0.8)
