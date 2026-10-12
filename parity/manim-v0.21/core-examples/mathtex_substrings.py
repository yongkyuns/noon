from manim import *


class MathTexSubstrings(Scene):
    def construct(self):
        equation = MathTex(
            r"x^2+y^2=z^2",
            substrings_to_isolate=("x", "y", "z"),
            font_size=64,
        )
        equation.set_color_by_tex("x", RED)
        equation.set_color_by_tex("y", GREEN)
        equation.set_color_by_tex("z", BLUE)
        self.add(equation)
        self.wait(.2)
