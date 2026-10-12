"""Native Text substring and source-range color coverage."""

from noon import Scene, Text


class TextRangeColors(Scene):
    def construct(self):
        label = Text(
            "Noon  café\nNoon Ω",
            font="DejaVu Sans Mono",
            font_size=42,
            t2c={"No": "#EF4444", "[6:10]": "#3B82F6", "[-1:]": "#22C55E"},
        )
        self.add(label)
        self.wait(0.2)
