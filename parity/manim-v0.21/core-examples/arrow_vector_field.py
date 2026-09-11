from manim import *


class ArrowVectorFieldStatic(Scene):
    def construct(self):
        field = ArrowVectorField(
            lambda point: point[0] * UP - point[1] * RIGHT,
            x_range=[-2.0, 2.0, 1.0],
            y_range=[-1.0, 1.0, 1.0],
        )
        self.add(field)
        self.wait(0.2)
