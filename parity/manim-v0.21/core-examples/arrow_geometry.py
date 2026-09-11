from manim import *


class ArrowGeometry(Scene):
    def construct(self):
        arrow = Arrow(
            (-3.0, 1.5, 0.0),
            (-0.5, 1.5, 0.0),
            buff=0.2,
            color="#58C4DD",
        ).scale(0.65)
        vector = (
            Vector((2.0, 1.0, 0.0), color="#F7D96F")
            .scale(0.65, scale_tips=True)
            .shift((0.0, -0.5, 0.0))
        )
        short = Arrow(
            (-0.2, 0.25, 0.0),
            (0.2, 0.25, 0.0),
            buff=0.0,
            color="#83C167",
        ).scale(3.0)
        double = DoubleArrow(
            (-2.5, -1.5, 0.0),
            (2.5, -1.5, 0.0),
            buff=0.15,
            color="#FC6255",
        ).scale(0.65)
        self.add(arrow, vector, short, double)
        self.wait(0.2)
