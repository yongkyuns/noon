"""Small real-Noon test of the same range-color API used by the full scene.

Paste into the Noon Playground. This file has not been rendered in the package
creation environment; it is provided for quick manual capability verification.
"""
from noon import *


class SyntaxHighlightingSmoke(Scene):
    def construct(self):
        source = "__shared__ float tile[32][33];"
        tokens = (
            ("__shared__", color_from_hex("#BAABFF")),
            ("float", color_from_hex("#BAABFF")),
            ("tile", color_from_hex("#67DAEE")),
            ("32", color_from_hex("#F2C27B")),
            ("33", color_from_hex("#8CE2B5")),
        )
        ranges = {}
        for token, color in tokens:
            start = source.index(token)
            ranges["[%d:%d]" % (start, start + len(token))] = color
        code = Text(source, font="DejaVu Sans Mono", font_size=30,
                    color=color_from_hex("#EDF3FC"), t2c=ranges)
        if code.width > 12:
            code.scale(12 / code.width)
        self.play(Write(code), run_time=1.0)
        self.wait(3.0)
