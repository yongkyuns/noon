"""Pinned common number-line geometry; no TeX or implicit font dependency."""
from manim import *


class NumberLineTicks(Scene):
    def construct(self):
        interval = UnitInterval(unit_size=6, color=GREEN, stroke_width=80 / 27)
        interval.shift(1.2 * UP)
        selected = NumberLine(
            [1000, 1001, 0.1], length=7,
            numbers_with_elongated_ticks=[1000.000000001, 1000.500001, 1001],
            longer_tick_multiple=3, color=BLUE, stroke_width=80 / 27,
        )
        selected.rotate(-0.18).shift(1.2 * DOWN)
        self.add(interval, selected)
        self.wait(0.2)
