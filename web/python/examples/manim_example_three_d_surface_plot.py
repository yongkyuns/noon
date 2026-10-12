# Noon capability adaptation: portable math and explicit point lighting.
# This does not claim Cairo shading parity with the pinned Manim reference.
import math

from noon import *


class ThreeDSurfacePlot(ThreeDScene):
    def construct(self):
        resolution_fa = 24
        self.set_camera_orientation(phi=75 * DEGREES, theta=-30 * DEGREES)

        def param_gauss(u, v):
            x = u
            y = v
            sigma, mu = 0.4, [0.0, 0.0]
            d = math.hypot(x - mu[0], y - mu[1])
            z = math.exp(-(d ** 2 / (2.0 * sigma ** 2)))
            return ([x, y, z])

        gauss_plane = Surface(
            param_gauss,
            shade_in_3d=False, point_lit=True,
            resolution=(resolution_fa, resolution_fa),
            v_range=[-2, +2],
            u_range=[-2, +2]
        )

        gauss_plane.scale(2, about_point=ORIGIN)
        gauss_plane.set_style(fill_opacity=1,stroke_color=GREEN)
        gauss_plane.set_fill_by_checkerboard(ORANGE, BLUE, opacity=0.5)
        axes = ThreeDAxes()
        self.renderer.camera.light_source
        self.add(axes,gauss_plane)
