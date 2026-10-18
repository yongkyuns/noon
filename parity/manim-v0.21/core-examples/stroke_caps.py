from manim import *


class StrokeCaps(Scene):
    def construct(self):
        caps = (
            CapStyleType.AUTO,
            CapStyleType.ROUND,
            CapStyleType.BUTT,
            CapStyleType.SQUARE,
        )
        colors = (BLUE, TEAL, GREEN, YELLOW)
        for index, (cap, color) in enumerate(zip(caps, colors, strict=True)):
            y = 1.2 - index * 0.8
            line = VMobject(color=color, stroke_width=20, fill_opacity=0)
            line.set_points_as_corners([(-2.5, y, 0), (2.5, y, 0)])
            line.set_cap_style(cap)
            self.add(line)
        self.wait(0.2)
