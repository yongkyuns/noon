"""Reveal compiled LaTeX glyphs and a fraction rule with one retained Create plan."""

from noon import *


class LatexCreate(Scene):
    async def construct(self):
        await prepare_latex()
        equation = MathTex(r"x^2+\frac{1}{2}", font_size=88, color=BLUE).shift(0.5 * DOWN)
        await self.play(Create(equation), run_time=2.0, rate_func=linear)
        await self.play(equation.animate.shift(UP), run_time=0.8, rate_func=linear)
        await self.wait(0.7)
