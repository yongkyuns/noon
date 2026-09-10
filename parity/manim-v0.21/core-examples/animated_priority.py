from manim import *


class OrdinaryAnimatedPriority(Scene):
    def construct(self):
        back = Square(side_length=2, color=BLUE, fill_opacity=1, stroke_width=0)
        front = Square(side_length=2, color=RED, fill_opacity=1, stroke_width=0).shift(0.5 * RIGHT + 0.5 * UP)
        self.add(back, front)
        self.play(back.animate.set_z_index(2), run_time=1, rate_func=linear)
        self.wait(0.5)
        self.play(back.animate.set_z_index(-1), run_time=1, rate_func=linear)
        self.wait(0.5)
        self.play(AnimationGroup(
            back.animate(run_time=0.5).set_z_index(2),
            front.animate(run_time=1.5).shift(LEFT),
            rate_func=linear,
        ))
        self.wait(0.5)
