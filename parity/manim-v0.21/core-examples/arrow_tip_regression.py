"""Identical source for the pinned Cairo oracle and Noon's Python facade.

No fonts, external packages, hand-drawn replacement tips, or renderer workarounds.
The oracle retains case names only to label read-only per-tip raster crops.
"""
from manim import *
import math

# name, kind, length, angle (degrees), tip length, stroke width, buff,
# max tip/length ratio, scale factor, scale_tips
ARROW_TIP_CASES = (
    ("horizontal", "arrow", 2.2, 0, 0.35, 6, 0, 0.25, 1, False),
    ("shallow", "arrow", 2.2, 7, 0.35, 6, 0, 0.25, 1, False),
    ("diagonal", "arrow", 2.0, 33, 0.35, 6, 0, 0.25, 1, False),
    ("vertical", "arrow", 1.0, 90, 0.22, 6, 0, 0.25, 1, False),
    ("reverse", "arrow", 2.2, 173, 0.35, 6, 0, 0.25, 1, False),
    ("small-tip", "arrow", 2.0, 11, 0.08, 3, 0, 0.25, 1, False),
    ("large-tip", "arrow", 2.2, -17, 0.55, 6, 0, 0.50, 1, False),
    ("short-cap", "arrow", 0.4, 27, 0.35, 6, 0, 0.25, 1, False),
    ("post-buff-cap", "arrow", 0.8, -23, 0.35, 6, 0.25, 0.25, 1, False),
    ("thin-shaft", "arrow", 2.0, -7, 0.35, 1, 0, 0.25, 1, False),
    ("thick-shaft", "arrow", 2.0, 17, 0.35, 9, 0, 0.25, 1, False),
    ("vector", "vector", 1.4, -33, 0.22, 4, 0, 0.25, 1, False),
    ("double", "double", 2.0, 0, 0.35, 6, 0, 0.25, 1, False),
    ("double-short", "double", 0.4, -27, 0.35, 6, 0, 0.25, 1, False),
    ("scale-keep-tips", "arrow", 2.0, 13, 0.35, 6, 0, 0.25, 0.5, False),
    ("scale-with-tips", "arrow", 2.0, -13, 0.35, 6, 0, 0.25, 0.5, True),
)

MOTION_SHIFT = (0.03, 0.018, 0)
MOTION_DURATION = 0.5


def make_arrow_tip_cases():
    result = []
    for index, case in enumerate(ARROW_TIP_CASES):
        name, kind, length, degrees, tip, stroke, buff, ratio, factor, scale_tips = case
        angle = math.radians(degrees)
        dx, dy = length * math.cos(angle), length * math.sin(angle)
        cx, cy = -4.8 + 3.2 * (index % 4), 2.4 - 1.6 * (index // 4)
        start = (cx - dx / 2, cy - dy / 2, 0)
        end = (cx + dx / 2, cy + dy / 2, 0)
        options = dict(color=WHITE, tip_length=tip, stroke_width=stroke, buff=buff,
                       max_tip_length_to_length_ratio=ratio)
        if kind == "vector":
            arrow = Vector((dx, dy, 0), **options).shift(start)
        else:
            constructor = DoubleArrow if kind == "double" else Arrow
            arrow = constructor(start, end, **options)
        if factor != 1:
            arrow.scale(factor, scale_tips=scale_tips)
        result.append((name, arrow))
    return result


class ArrowTipRegression(Scene):
    def construct(self):
        self.arrow_tip_cases = make_arrow_tip_cases()
        # Create the shared family before its member Arrows are admitted to the
        # scene so both authoring hosts see the same stable membership graph.
        motion_group = VGroup(*(arrow for _, arrow in self.arrow_tip_cases))
        self.add(*(arrow for _, arrow in self.arrow_tip_cases))
        self.wait(0.25)
        # About two pixels of motion at DPR=1: exercise coverage changes rather
        # than letting a convenient static pixel alignment hide jagged edges.
        # One common VGroup shift preserves that same per-arrow geometry while
        # presenting it as one family request to the shared runtime.
        self.play(
            motion_group.animate.shift(MOTION_SHIFT),
            run_time=MOTION_DURATION,
            rate_func=linear,
        )
        self.wait(0.25)
