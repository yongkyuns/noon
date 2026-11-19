"""Animate a callout that draws its outline and owns the label inside it."""
from noon import *


class MountedCallout(Scene):
    def construct(self):
        ink = color_from_hex("#9bf4e2")
        rail = Rectangle(width=4.5, height=0.035, fill="#415874", fill_opacity=1,
                         stroke=None).shift(0.75 * DOWN)
        self.add(rail)
        for x, name in [(-2.25, "Sample A"), (2.25, "Sample B")]:
            marker = Circle(radius=0.12, fill=ink, fill_opacity=1, stroke=None)
            marker.shift(x * RIGHT + 0.75 * DOWN)
            text = Text(name, font_size=27, color=ink).shift(x * RIGHT + 1.25 * DOWN)
            self.add(marker, text)

        label = Text("Inspect", font_size=40, color=ink).shift(0.8125 * UP)
        corners = [
            (-1.5, 0.25, 0), (-1.5, 1.25, 0), (-1.375, 1.375, 0), (1.375, 1.375, 0),
            (1.5, 1.25, 0), (1.5, 0.25, 0), (0.25, 0.25, 0), (0, -0.125, 0),
            (-0.25, 0.25, 0), (-1.5, 0.25, 0),
        ]
        callout = VGroup().set_points([
            [(1 - t) * a + t * b for a, b in zip(start, end)]
            for start, end in zip(corners, corners[1:])
            for t in (0, 1 / 3, 2 / 3, 1)
        ]).add(label)
        callout.set_fill(color_from_hex("#183846"), opacity=1, family=False)
        callout.set_stroke(ink, width=6, family=False)
        callout.shift(9 * LEFT)
        self.wait(0.5)
        self.play(callout.animate.shift(11.25 * RIGHT), run_time=1, rate_func=linear)
        self.wait(0.5)
        self.play(callout.animate.scale(1.25), run_time=1, rate_func=linear)
        self.wait(1.0)
