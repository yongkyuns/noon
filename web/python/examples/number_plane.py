"""Major/faded grid lines, a function, and a marker in one retained family.

The curve snapshots the plane's coordinates. Group it with the plane and marker
when they should move together; no callbacks or per-frame grid rebuilds are used.
"""
from noon import *


class NumberPlaneExample(Scene):
    def construct(self):
        plane = NumberPlane(
            x_range=[-3, 3, 1], y_range=[-2, 2, 1], x_length=8, y_length=4.5,
            background_line_style={"stroke_color": GREEN, "stroke_width": 1.0},
            faded_line_ratio=2,
        )
        curve = plane.plot(lambda x: 0.35 * x * x - 1, [-3, 3, 0.05], color=BLUE)
        point = plane.c2p(1, -0.65)
        roundtrip = plane.p2c(point)
        assert abs(roundtrip[0] - 1) < 1e-6 and abs(roundtrip[1] + 0.65) < 1e-6
        marker = Dot(point, color=YELLOW)
        content = VGroup(plane, curve, marker).scale(0.85).shift(0.1 * DOWN)
        title = Text("NumberPlane: a shared coordinate grid", font_size=28).shift(3 * UP)
        caption = Text("Major lines + subdivisions | y = 0.35 x² - 1 | point (1, -0.65)", font_size=20).shift(3 * DOWN)
        self.add(content, title, caption)
