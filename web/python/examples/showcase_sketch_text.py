"""Draw sketched glyph outlines with the original text and equation layout."""
from noon import *


class SketchText(Scene):
    def construct(self):
        title = Text("Noon", font_size=120, color=BLUE_E,
                     stroke=SketchStroke(TEAL, roughness=1.1, seed=7)).shift(1.35 * UP)
        equation = MathTex(r"\frac{a^2+b^2}{2}", font_size=110, color=BLUE_E,
                           stroke=SketchStroke(YELLOW, roughness=1.1, seed=12),
                           stroke_width=2.8).move_to((0, -1.10, 0))
        self.play(Create(title), run_time=1.2, rate_func=linear)
        self.play(Write(equation, lag_ratio=0.12), run_time=1.2, rate_func=linear)
        self.wait(0.6)
