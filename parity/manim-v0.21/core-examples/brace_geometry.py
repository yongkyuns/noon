from manim import *


class BraceGeometry(Scene):
    def construct(self):
        square = Square(2.0).shift(LEFT * 2.0)
        lower = Brace(square, direction=DOWN, buff=0.2, color=BLUE)

        rectangle = Rectangle(width=2.5, height=1.5).shift(RIGHT * 2.0)
        side = Brace(rectangle, direction=RIGHT, buff=0.25, color=YELLOW)

        between = BraceBetweenPoints(
            (-1.25, 2.0, 0.0),
            (1.25, 2.0, 0.0),
            direction=UP,
            buff=0.15,
            sharpness=1.5,
            color=RED,
        )

        self.add(square, lower, rectangle, side, between)
        self.wait(0.2)
