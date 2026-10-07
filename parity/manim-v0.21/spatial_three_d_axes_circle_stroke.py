"""Pinned ManimCE 0.21 Circle screen-stroke camera-motion oracle."""

import numpy as np
from manim import *


class SpatialCircleScreenStroke(ThreeDScene):
    def construct(self):
        self.set_camera_orientation(
            phi=0.6, theta=-1.2, gamma=0, focal_distance=5, zoom=1,
            frame_center=ORIGIN,
        )
        circle = Circle(radius=0.65, color=WHITE)
        circle.set_fill(opacity=0)
        circle.set_stroke(WHITE, width=4)
        circle.scale(np.array([1.1, 0.7, 1.0]))
        circle.rotate(0.45, axis=UP)
        circle.shift(np.array([-1.3, 1.35, 0.0]))
        self.add(circle)
        self.move_camera(
            phi=0.8, theta=-0.1, gamma=0.2, focal_distance=5, zoom=1.1,
            frame_center=(0.3, 0, 0), run_time=1, rate_func=linear,
        )
