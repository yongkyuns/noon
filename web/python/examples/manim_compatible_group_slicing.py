from noon import *


class GroupSlicing(Scene):
    def construct(self):
        first = Square(side_length=0.5).shift(LEFT * 2.0)
        middle = Circle(radius=0.3)
        last = Rectangle(width=0.8, height=0.4).shift(RIGHT * 2.0)
        family = VGroup(first, middle, last)

        self.add(*family)
        self.play(family[1:].animate(run_time=1.0).shift(UP * 0.75))
