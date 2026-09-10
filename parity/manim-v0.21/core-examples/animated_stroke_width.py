from manim import *


class OrdinaryAnimatedStrokeWidth(Scene):
    def construct(self):
        shape = Circle(radius=1, color=WHITE, stroke_width=0)
        self.add(shape)
        self.play(shape.animate.set_stroke(width=20), run_time=1, rate_func=linear)
        self.play(shape.animate.set_stroke(width=2), run_time=1, rate_func=linear)
