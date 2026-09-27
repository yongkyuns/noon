"""Create two objects, arrange them, and animate a shared composition."""
from noon import *


class FirstScene(Scene):
    def construct(self):
        title = Text("Your first scene", font_size=36).shift(2.7 * UP)
        caption = Text("Create objects, then give them a little motion", font_size=22).shift(2.5 * DOWN)
        circle = Circle(radius=0.75, color=BLUE).set_fill(BLUE, opacity=0.8)
        square = Square(side_length=1.5, color=PINK).set_fill(PINK, opacity=0.8)
        circle.shift(1.5 * LEFT)
        square.next_to(circle, RIGHT, buff=1.5)

        self.play(FadeIn(title), FadeIn(caption), run_time=0.7, rate_func=smooth)
        self.play(Create(circle), Create(square), run_time=1.4, rate_func=smooth)
        self.wait(0.5)
        self.play(
            circle.animate.shift(0.7 * UP),
            square.animate.rotate(PI / 4).shift(0.7 * DOWN),
            run_time=1.6, rate_func=smooth,
        )
        self.play(
            circle.animate.shift(0.7 * DOWN),
            square.animate.rotate(-PI / 4).shift(0.7 * UP),
            run_time=1.6, rate_func=smooth,
        )
        self.play(Indicate(circle), Indicate(square), run_time=1.0)
        self.wait(1.2)
