"""Animated pair of noon::animated_number_line_example::program()."""
from noon import *


class AnimatedNumberLine(Scene):
    def construct(self):
        title = Text("NumberLine: position and direction", font_size=28).shift(3 * UP)
        caption = Text("Coordinates follow the line", font_size=22).shift(2 * UP)
        number_line = NumberLine([-4, 4, 1], length=8.8, include_ticks=True, color=WHITE)
        number_line.add_numbers(font_size=18, exclude_zero=False)
        marker = Dot(number_line.n2p(-4), color=YELLOW, radius=0.11)

        self.play(Write(title), run_time=0.5, rate_func=linear)
        self.play(Create(number_line), run_time=1.0, rate_func=linear)
        self.play(
            FadeIn(marker, run_time=0.5), Write(caption, run_time=0.5),
            run_time=0.5, rate_func=linear,
        )
        self.play(marker.animate.move_to(number_line.n2p(0)), run_time=1.5, rate_func=linear)
        self.wait(0.5)
        self.play(marker.animate.move_to(number_line.n2p(3)), run_time=1.2, rate_func=linear)
        self.wait(0.5)

        # Live n2p reads the completed Rust-owned coordinate frame, including
        # this transform. No cached targets or Python coordinate arithmetic.
        self.play(
            number_line.animate.shift(0.6 * UP),
            marker.animate.shift(0.6 * UP),
            run_time=0.8, rate_func=linear,
        )
        target = number_line.n2p(-2)
        assert abs(number_line.p2n(target) + 2) < 2e-5
        self.play(marker.animate.move_to(target), run_time=1.5, rate_func=linear)
        self.wait(0.8)
