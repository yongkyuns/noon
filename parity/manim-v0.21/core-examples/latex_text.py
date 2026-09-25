from manim import *


class LatexTextExample(Scene):
    def construct(self):
        title = Tex(r"Real \LaTeX{} in Noon", font_size=38).move_to((0, 2, 0))
        fraction = SingleStringMathTex(r"x^2+\frac{1}{2}", font_size=64, color=BLUE).move_to((0, 0.5, 0))
        equation = MathTex(
            r"\alpha+\Gamma+\sum_{i=1}^{3}i+\sqrt{2}", font_size=48, color=YELLOW,
        ).move_to((0, -1.3, 0))
        self.add(title, fraction, equation)
        self.wait(.2)
