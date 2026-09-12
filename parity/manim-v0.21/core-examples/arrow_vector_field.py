from manim import *


def sparse_default_field(point):
    x = point[0]
    if x == -6.0:
        return 0.25 * RIGHT
    if x == 0.0:
        return 1.0 * UP
    if x == 6.0:
        return 2.0 * LEFT
    return ORIGIN


class ArrowVectorFieldStatic(Scene):
    def construct(self):
        field = ArrowVectorField(
            sparse_default_field,
            y_range=[0.0, 0.0, 1.0],
        )
        self.add(field)
        self.wait(0.2)
