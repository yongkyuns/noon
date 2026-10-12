"""Paired with the shared NumberPlane native/direct-WASM factory.

Complex coordinate conversion delegates to the same retained Rust Cartesian
frame as NumberPlane; the resulting geometry and presentation stay paired.
"""
from noon import *


class ComplexPlaneExample(Scene):
    def construct(self):
        plane = ComplexPlane(
            x_range=[-3, 3, 1], y_range=[-2, 2, 1], x_length=8, y_length=4.5,
            background_line_style={"stroke_color": GREEN, "stroke_width": 1.0},
            faded_line_ratio=2,
        )
        curve = plane.plot(lambda x: 0.35 * x * x - 1, [-3, 3, 0.05], color=BLUE)
        point = plane.n2p(1 - 0.65j)
        roundtrip = plane.p2n(point)
        assert abs(roundtrip - (1 - 0.65j)) < 1e-6
        marker = Dot(point, color=YELLOW)
        content = VGroup(plane, curve, marker).scale(0.85).shift(0.1 * DOWN)
        title = Text("NumberPlane: a shared coordinate grid", font_size=28).shift(3 * UP)
        caption = Text(
            "Major lines + subdivisions | y = 0.35 x² - 1 | point (1, -0.65)",
            font_size=20,
        ).shift(3 * DOWN)
        self.add(content, title, caption)
