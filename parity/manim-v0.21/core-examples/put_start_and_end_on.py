from manim import *


class PutStartAndEndOn(Scene):
    def construct(self):
        diagonal = Line((-1.5, -0.5, 0), (0.5, 1, 0), color=BLUE)
        diagonal.scale(0.75, about_point=(-1.5, -0.5, 0))
        diagonal.rotate(0.2, about_point=ORIGIN)
        diagonal.put_start_and_end_on((1, -1, 0), (-0.5, 1.5, 0))

        horizontal = Line((-0.5, 0.75, 0), (0.75, -0.25, 0), color=RED)
        horizontal.put_start_and_end_on((2, 1, 0), (4, 1, 0))
        self.add(diagonal, horizontal)
        self.wait(0.2)
