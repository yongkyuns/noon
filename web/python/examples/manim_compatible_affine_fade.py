from noon import *


class AffineFadeDemo(Scene):
    def construct(self):
        circle = (
            Circle(radius=0.6)
            .set_fill(BLUE, opacity=1.0)
            .set_stroke(opacity=0.0)
        )

        self.play(
            FadeIn(circle, shift=2 * RIGHT, scale=0.25),
            run_time=1.0,
            rate_func=linear,
        )
        self.wait(0.25)
        self.play(
            FadeOut(circle, target_position=2 * RIGHT, scale=0.15),
            run_time=1.0,
            rate_func=linear,
        )
