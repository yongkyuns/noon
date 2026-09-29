"""Introduce a fixed click target beside a rectangle that can be dragged."""
from noon import *


class TranslationDrag(Scene):
    def construct(self):
        title = Text("Move a shape", font_size=36).shift(2.7 * UP)
        instructions = Text("Click the blue circle. Drag the green rectangle.", font_size=22).shift(2.5 * DOWN)
        circle = Circle(radius=0.9, color=BLUE).set_fill(BLUE, opacity=0.78).shift(2 * LEFT)
        rectangle = Rectangle(width=2.2, height=1.6, color=GREEN).set_fill(GREEN, opacity=0.78).shift(2 * RIGHT)
        self.play(FadeIn(title), FadeIn(instructions), run_time=0.7)
        self.play(Create(circle), Create(rectangle), run_time=1.4, rate_func=smooth)
        self.wait(0.5)
        self.on_click(circle, Indicate(circle, run_time=0.4))
        self.set_drag_targets(rectangle)
