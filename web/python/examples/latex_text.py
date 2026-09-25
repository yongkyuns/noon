from noon import *

class LatexTextExample(Scene):
    async def construct(self):
        # Optional preparation finishes before ordinary synchronous authoring.
        await prepare_latex()
        title = Tex(r"Real \LaTeX{} in Noon", font_size=38).move_to((0, 2))
        fraction = SingleStringMathTex(r"x^2+\frac{1}{2}", font_size=64, color=BLUE).move_to((0, 0.5))
        equation = MathTex(
            r"\alpha+\Gamma+\sum_{i=1}^{3}i+\sqrt{2}", font_size=48, color=YELLOW,
        ).move_to((0, -1.3))
        self.add(title, fraction, equation)
