from manim import *


class NumberLineDefaults(Scene):
    def construct(self):
        default = NumberLine().shift(2 * UP)
        partial = NumberLine([-2, 2])
        explicit = NumberLine([-2, 2, 0.5]).shift(2 * DOWN)
        self.add(default, partial, explicit)
        self.wait(0.2)
