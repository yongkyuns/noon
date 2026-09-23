"""Combine animated native text, inline styling, and actual typeset mathematics."""
from noon import *


class TextAndMathematics(Scene):
    def construct(self):
        title = Text("Words and equations", font_size=36).shift(2.8 * UP)
        styled = MarkupText('Make the <b>important</b> idea <span foreground="#58c4dd">stand out</span>.', font_size=30).shift(1.3 * UP)
        equation = MathTypst(r"sum_(k=1)^n k = frac(n(n + 1), 2)", font_size=60).shift(0.3 * DOWN)
        caption = Text("Native text + inline emphasis + Typst mathematics", font_size=22).shift(2.4 * DOWN)
        self.play(Write(title), run_time=1.2, rate_func=smooth)
        self.play(FadeIn(styled, shift=0.2 * UP), run_time=1.0)
        self.wait(0.5)
        self.play(FadeIn(equation, shift=0.2 * UP), run_time=1.2)
        self.play(Write(caption), run_time=1.2)
        self.wait(1.4)
