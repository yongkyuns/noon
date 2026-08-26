from noon import *


class GrowFromCenterExample(Scene):
    def construct(self):
        squares = [Square() for _ in range(2)]
        VGroup(*squares).set_x(0).arrange(buff=2)
        self.play(GrowFromCenter(squares[0]))
        self.play(GrowFromCenter(squares[1], point_color=RED))
