from manim import *


class UsingFocusOn(Scene):
    def construct(self):
        dot = Dot(color=PURE_YELLOW).shift(DOWN)
        self.add(Tex("Focusing on the dot below:"), dot)
        self.play(FocusOn(dot))
        self.wait()
