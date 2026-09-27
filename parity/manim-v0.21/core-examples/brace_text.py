"""Pinned ManimCE v0.21 source pair for Noon's BraceText fixture."""
from manim import *


class BraceTextExample(Scene):
    def construct(self):
        # Pin the font so the demonstration is portable across native hosts.
        def label(text, font_size):
            return Text(text, font_size=font_size, font="DejaVu Sans Mono")

        left = Square().shift(LEFT * 2.0)
        left_label = BraceText(left, "Label", font_size=36, label_constructor=label)

        right = Rectangle(width=2.5, height=1.5).shift(RIGHT * 2.0)
        right_label = BraceText(
            right,
            "Side",
            label_constructor=label,
            brace_direction=RIGHT,
            font_size=36,
            buff=0.25,
        )

        self.add(left, left_label, right, right_label)
        self.wait(0.2)
