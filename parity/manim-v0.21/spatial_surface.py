from manim import *
import math
import numpy as np


class SpatialSurface(ThreeDScene):
    def construct(self):
        self.set_camera_orientation(phi=0, theta=-90 * DEGREES,
                                    focal_distance=5, zoom=4 / (5 * math.tan(0.5)))
        surface = Surface(
            lambda u, v: [u, v, 0.25 * u * v],
            u_range=(-1.5, 1.5), v_range=(-1.5, 1.5), resolution=(8, 8),
            checkerboard_colors=False, fill_color=ManimColor((0.2, 0.55, 0.85)),
            fill_opacity=1, stroke_width=0, shade_in_3d=False,
        )
        surface.set_shade_in_3d(False)
        self.add(surface)
        self.play(Rotate(surface, 0.6, axis=OUT, about_point=ORIGIN),
                  run_time=1, rate_func=linear)


class CairoSpatialSurface(ThreeDScene):
    def construct(self):
        self.set_camera_orientation(phi=0, theta=-90 * DEGREES,
                                    focal_distance=5, zoom=4 / (5 * math.tan(0.5)))
        surface = Surface(
            lambda u, v: [u, v, 0.35 * (u * u + v * v)],
            u_range=(-1, 1), v_range=(-1, 1), resolution=(8, 8),
        )
        self.add(surface)


class CairoSphereTorus(ThreeDScene):
    def construct(self):
        self.set_camera_orientation(
            phi=0.6, theta=-1.2, gamma=0, focal_distance=5, zoom=1,
            frame_center=(0, 0, 0),
        )
        sphere = Sphere(center=(-1.2, 0, 0), radius=0.6)
        torus = Torus(major_radius=0.6, minor_radius=0.2).shift((1.2, 0, 0))
        self.add(sphere, torus)
        self.move_camera(
            phi=0.8, theta=-0.1, gamma=0.2, zoom=1.1,
            frame_center=(0.3, 0, 0), run_time=1, rate_func=linear,
        )


class CairoConeBodies(ThreeDScene):
    def construct(self):
        self.set_camera_orientation(
            phi=0, theta=-90 * DEGREES, focal_distance=5,
            zoom=4 / (5 * math.tan(0.5)),
        )
        default_cone = Cone().shift((-1.25, 0, 0))
        tilted_partial_cone = Cone(
            base_radius=0.8, height=1.4, direction=np.array([1, 2, 2]), u_min=0.2,
        ).shift((1.25, 0, 0))
        self.add(default_cone, tilted_partial_cone)
