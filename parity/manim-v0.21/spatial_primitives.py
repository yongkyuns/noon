"""Pinned Manim counterpart to Noon's retained spatial primitives scene."""

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
        prism = Prism(
            dimensions=(1.0, 0.7, 0.5), fill_color=BLUE, fill_opacity=0.75,
            stroke_width=0, shade_in_3d=False,
        ).shift((1.6, -1.9, 0.25))
        cylinder = Cylinder(
            radius=0.2, height=1.1, direction=np.array([1, 2, 1]), resolution=16,
            fill_color=GREEN, checkerboard_colors=False, stroke_width=0,
            shade_in_3d=False,
        ).shift((-1.2, 0.8, 0))
        open_cylinder = Cylinder(
            radius=0.2, height=1.1, direction=np.array([1, 2, 1]), show_ends=False,
            resolution=16, fill_color=TEAL, checkerboard_colors=False, stroke_width=0,
            shade_in_3d=False,
        ).shift((-0.55, 0.8, 0))
        cone = Cone(
            base_radius=0.25, height=0.9, direction=np.array([-2, 1, -1]), show_base=True,
            resolution=16, fill_color=YELLOW, stroke_width=0, shade_in_3d=False,
        ).shift((0.85, 0.8, 0))
        open_cone = Cone(
            base_radius=0.25, height=0.9, direction=np.array([-2, 1, -1]), show_base=False,
            resolution=16, fill_color=RED, stroke_width=0, shade_in_3d=False,
        ).shift((1.75, 0.8, 0))
        sphere_patch = Sphere(
            center=np.array([0, 1.9, 0]), radius=0.35, resolution=(24, 12),
            u_range=(PI / 4, 3 * PI / 4), v_range=(PI / 6, 5 * PI / 6),
            fill_color=PINK, checkerboard_colors=False, stroke_width=0,
            shade_in_3d=False,
        )
        self.add(line, triangle, prism, cylinder, open_cylinder, cone, open_cone, sphere_patch)
        self.play(
            line.animate.shift(0.25 * OUT),
            triangle.animate.rotate(0.4, axis=UP, about_point=ORIGIN),
            run_time=1, rate_func=linear,
        )
