"""Paired Arrow/Vector/DoubleArrow example over the shared Rust family path."""
from noon import *


class ArrowGeometry(Scene):
    def construct(self):
        arrow = Arrow(
            (-3.0, 1.5, 0.0),
            (-0.5, 1.5, 0.0),
            buff=0.2,
            color="#58C4DD",
        )
        vector = Vector((2.0, 1.0, 0.0), color="#F7D96F").shift((0.0, -0.5, 0.0))
        double = DoubleArrow(
            (-2.5, -1.5, 0.0),
            (2.5, -1.5, 0.0),
            buff=0.15,
            color="#FC6255",
        )
        self.add(arrow, vector, double)
        self.wait(0.2)