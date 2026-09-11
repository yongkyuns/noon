# Source-equivalent ManimCE v0.21.0 / Noon native Text range-color probe.
# The raster harness changes only the import line and appends its scene-selection wrapper.

from manim import *


class TextRangeColors(Scene):
    def construct(self):
        label = Text(
            "Noon blue Noon",
            font="DejaVu Sans Mono",
            font_size=48,
            t2c={"Noon": RED, "[5:9]": BLUE},
        )
        self.add(label)
        self.wait(1.0)
