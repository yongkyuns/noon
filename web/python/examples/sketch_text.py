"""Draw sketched glyph outlines with the original text and equation layout."""
from noon import *


class SketchText(Scene):
    async def construct(self):
        await prepare_latex()
        title = Text("Noon", font_size=120, color=BLUE_E,
                     stroke=SketchStroke(TEAL, roughness=1.1, seed=7)).shift(1.35 * UP)
        equation = MathTex(r"\frac{a^2+b^2}{2}", font_size=110, color=BLUE_E,
                           stroke=SketchStroke(YELLOW, roughness=1.1, seed=12),
                           stroke_width=2.8).move_to((0, -1.10, 0))
        await self.play(Create(title), Write(equation, lag_ratio=0.12), run_time=2.4, rate_func=linear)
        await self.wait(0.6)
