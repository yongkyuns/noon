"""Paired Arc/ArcBetweenPoints example over the shared Rust geometry path."""
from noon import *


class ArcGeometry(Scene):
    def construct(self):
        arc = Arc(
            radius=1.25,
            start_angle=-0.3,
            angle=1.8,
            num_components=9,
            arc_center=(-2.0, 0.8, 0.0),
            color="#58C4DD",
        )
        between = ArcBetweenPoints(
            (-0.5, -1.5, 0.0),
            (2.5, 1.0, 0.0),
            angle=PI / 2,
            color="#F7D96F",
        )
        negative_radius = ArcBetweenPoints(
            (0.5, -2.0, 0.0),
            (3.0, -2.0, 0.0),
            radius=-2.0,
            color="#FC6255",
        )
        self.add(arc, between, negative_radius)
        self.wait(0.2)
