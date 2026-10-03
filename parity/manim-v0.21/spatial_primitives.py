"""Pinned Manim counterpart to Noon's retained Line3D/polyhedron scene."""

import math
import numpy as np
from manim import *


class SpatialPrimitives(ThreeDScene):
    def construct(self):
        self.set_camera_orientation(
            phi=0, theta=-PI / 2, focal_distance=5,
            zoom=4 / (5 * np.tan(0.5)), frame_center=ORIGIN,
        )
        self.camera.should_apply_shading = False
        line = Line3D(
            start=(-2.0, -0.8, 0.0), end=(-0.2, -0.8, 0.0),
            thickness=0.18, color=RED, resolution=16,
            checkerboard_colors=False, stroke_width=0, shade_in_3d=False,
        )
        triangle = Polygon(
            np.array([0.45, -1.0, 0.0]),
            np.array([2.25, -1.0, 0.0]),
            np.array([1.35, 1.0, 0.25]),
            fill_color=BLUE, fill_opacity=1, stroke_width=0, shade_in_3d=False,
        )
        self.add(line, triangle)
        self.play(
            line.animate.shift(0.25 * OUT),
            triangle.animate.rotate(0.4, axis=UP, about_point=ORIGIN),
            run_time=1, rate_func=linear,
        )
