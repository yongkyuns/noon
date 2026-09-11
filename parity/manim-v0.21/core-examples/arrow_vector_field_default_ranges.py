from manim import *


class ArrowVectorFieldDefaultRanges(Scene):
    def construct(self):
        field = ArrowVectorField(
            lambda point: 0.25 * RIGHT,
            color=PURPLE,
        )
        self.add(field)
        self.wait(0.2)
