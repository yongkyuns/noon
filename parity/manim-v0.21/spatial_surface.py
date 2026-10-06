from manim import *
import math


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
