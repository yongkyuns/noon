"""Native Text source-range colors with Manim-compatible ``t2c`` selectors."""

from noon import BLUE, RED, Scene, Text


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
