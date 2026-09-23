"""Scene portion of the pointer-selection example; the gallery supplies the documented host interaction."""
from noon import *


class PointerSelection(Scene):
    def construct(self):
        title = Text("Select a shape", font_size=36).shift(2.7 * UP)
        instructions = Text("After the reveal: click a shape. Click the background to clear.", font_size=20).shift(2.5 * DOWN)
        circle = Circle(radius=0.9, color=BLUE).set_fill(BLUE, opacity=0.75).shift(2 * LEFT)
        rectangle = Rectangle(width=2.2, height=1.6, color=GREEN).set_fill(GREEN, opacity=0.75).shift(2 * RIGHT)
        self.play(FadeIn(title), FadeIn(instructions), run_time=0.7)
        self.play(Create(circle), Create(rectangle), run_time=1.4, rate_func=smooth)
        self.wait(0.5)
