from manim import *


class HelloTypst(Scene):
    def construct(self):
        text = Typst(r"*Hello* from _Typst!_", font_size=96)
        self.add(text)


class HelloMathTypst(Scene):
    def construct(self):
        equation = MathTypst(r"sum_(k=1)^n k = (n(n + 1)) / 2", font_size=72)
        self.add(equation)
