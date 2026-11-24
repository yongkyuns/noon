"""Move a boxed equation as a group, then highlight its original first term."""
from noon import *


class EquationCard(Scene):
    def construct(self):
        self.add(Text("Highlight a term", font_size=32).shift(1.9 * UP))
        equation = MathTex("x", "+", "1", "=", "4", font_size=58).shift(0.6 * DOWN)
        term = equation[0]
        frame = Rectangle(width=3.8, height=1.3, color=BLUE).shift(0.6 * DOWN)
        card = VGroup(frame, equation)
        self.wait(0.5)
        self.play(card.animate.shift(0.85 * UP), run_time=0.75)
        self.wait(0.25)
        self.play(term.animate.set_color(YELLOW), run_time=0.5)
        self.wait(0.75)
