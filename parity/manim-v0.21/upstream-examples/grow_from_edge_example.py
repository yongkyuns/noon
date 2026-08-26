from manim import *


class GrowFromEdgeExample(Scene):
    def construct(self):
        squares = [Square() for _ in range(4)]
        VGroup(*squares).set_x(0).arrange(buff=1)
        self.play(GrowFromEdge(squares[0], DOWN))
        self.play(GrowFromEdge(squares[1], RIGHT))
        self.play(GrowFromEdge(squares[2], UR))
        self.play(GrowFromEdge(squares[3], UP, point_color=RED))
