"""Compare Create and Write on compiled LaTeX glyphs and a vector fraction rule."""

from noon import *


class LatexCreate(Scene):
    async def construct(self):
        await prepare_latex()
        drawn = MathTex(r"x^2+\frac{1}{2}", font_size=88, color=BLUE).shift(1.5 * UP)
        written = MathTex(r"x^2+\frac{1}{2}", font_size=88, color=YELLOW).shift(1.5 * DOWN)
        self.add(
            Text("Create()", font_size=28).shift(3.8 * LEFT + 1.5 * UP),
            Text("Write()", font_size=28).shift(3.8 * LEFT + 1.5 * DOWN),
        )
        await self.play(Create(drawn), Write(written), run_time=2.0, rate_func=linear)
        await self.wait(1.5)
