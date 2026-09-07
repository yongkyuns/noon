from noon import *


class OrdinaryDrawBorderThenFill(Scene):
    def construct(self):
        square = Square(side_length=0.8).set_fill(ORANGE, opacity=1).set_stroke(BLUE, width=6).shift(LEFT)
        circle = Circle(radius=0.4).set_fill(PINK, opacity=1).set_stroke(opacity=0).shift(RIGHT)
        self.play(Write(VGroup(square, circle), run_time=3, lag_ratio=0.5,
                        stroke_width=4, stroke_color=YELLOW))
        self.wait(0.25)
