"""Retained polar circles and rays with a shared Rust polar round trip."""
from math import pi

from manim import *


class PolarGridCoordinates(Scene):
    def construct(self):
        plane = PolarPlane(
            radius_max=3, size=6, radius_step=1, azimuth_step=12,
            azimuth_offset=pi / 6, azimuth_direction="CW", faded_line_ratio=2,
            background_line_style={"stroke_color": BLUE, "stroke_width": 1.0},
        )
        point = plane.polar_to_point(2, pi / 3)
        roundtrip = plane.point_to_polar(point)
        assert abs(roundtrip[0] - 2) < 1e-6
        marker = Dot(point, color=YELLOW)
        self.add(VGroup(plane, marker))
        self.wait(0.2)
