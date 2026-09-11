from manim import *


class ArrowVectorFieldColors(Scene):
    def construct(self):
        single = ArrowVectorField(
            lambda point: 0.75 * RIGHT + 0.25 * UP,
            color=PURPLE,
            x_range=[-4.0, -2.0, 1.0],
            y_range=[-1.0, 1.0, 1.0],
        )
        mapped = ArrowVectorField(
            lambda point: point[0] * UP - point[1] * RIGHT,
            color_scheme=lambda vector: vector[1],
            min_color_scheme_value=1.0,
            max_color_scheme_value=3.0,
            colors=[GREEN, YELLOW, RED],
            length_func=lambda norm: 0.35,
            x_range=[1.0, 3.0, 1.0],
            y_range=[-1.0, 1.0, 1.0],
        )
        self.add(single, mapped)
        self.wait(0.2)
