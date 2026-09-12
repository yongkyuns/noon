"""Paired Arrow/Vector/DoubleArrow example over the shared Rust family path."""
from noon import *


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
        source = Circle(radius=0.45).shift((2.45, 1.45, 0.0))
        target = Square(side_length=0.8).rotate(PI / 8).shift((3.8, 1.15, 0.0))
        bounded = Arrow(source, target, buff=0.05, color="#9A72AC")
        self.add(arrow, vector, short, double, source, target, bounded)
        self.wait(0.2)
