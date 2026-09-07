from noon import *


class OrdinaryDrawBorderThenFill(Scene):
    def construct(self):
        square = Square(side_length=0.8).set_fill(ORANGE, opacity=1).set_stroke(BLUE, width=6)
        circle = Circle(radius=0.4).set_fill(PINK, opacity=1).set_stroke(width=0)
        family = VGroup(square, circle).arrange(RIGHT, buff=1.2)
        self.play(Write(family, run_time=3, lag_ratio=0.5,
                        stroke_width=4, stroke_color="#F7D96F"))
        self.wait(0.25)
