# Noon capability adaptation: portable math and explicit point lighting.
# This does not claim Cairo shading parity with the pinned Manim reference.
import math

from noon import *


class ThreeDLightSourcePosition(ThreeDScene):
    def construct(self):
        axes = ThreeDAxes()
        sphere = Surface(
            lambda u, v: ([
                1.5 * math.cos(u) * math.cos(v),
                1.5 * math.cos(u) * math.sin(v),
                1.5 * math.sin(u)
            ]), v_range=[0, TAU], u_range=[-PI / 2, PI / 2],
            checkerboard_colors=[RED_D, RED_E], resolution=(15, 32),
            shade_in_3d=False, point_lit=True
        )
        self.renderer.camera.light_source.move_to([0, 0, -3])  # Match the native point-light profile.
        self.set_camera_orientation(phi=75 * DEGREES, theta=30 * DEGREES)
        self.add(axes, sphere)
