# Copyright (c) 2026 Trimora Inc.
# SPDX-License-Identifier: MIT
# Original: https://rendercomp.com/free/
# https://github.com/RenderComp/free-remotion-templates/blob/f648980e528defb7594e24c6447055f0540f340b/src/components/racing-chart/RacingChart.tsx
# See THIRD_PARTY_NOTICES.md.
"""RenderComp's Market Share Race. Each frame sorts the same five series."""

from math import cos, pi, sin

from noon import *


FPS = 30
FRAME_COUNT = 210
PIXELS_PER_UNIT = 135
DATA = (
    ("Aurora", (22, 28, 34, 41, 48), "#d4af37"),
    ("Beacon", (30, 32, 33, 35, 36), "#2563eb"),
    ("Cobalt", (18, 24, 30, 32, 38), "#10b981"),
    ("Delta", (26, 25, 24, 22, 20), "#ef4444"),
    ("Echo", (12, 16, 20, 28, 34), "#a855f7"),
)
BAR_LEFT = 196
BAR_TOP = 441
BAR_AVAILABLE = 1644
BAR_RADIUS = 24


def y_at(rank):
    return BAR_TOP + 24 + 64 * rank


def screen(x, y):
    return ((x - 960) / PIXELS_PER_UNIT, (540 - y) / PIXELS_PER_UNIT)


def current_values(frame):
    step = max(0, min(4, (frame - 18) / 36))
    left = int(step)
    right = min(left + 1, 4)
    return [values[left] + (values[right] - values[left]) * (step - left)
            for _, values, _ in DATA]


def row_rank(values, series):
    # The source uses a stable descending sort. Exact ties retain source order.
    return sum(1 for i, value in enumerate(values)
               if value > values[series] or (value == values[series] and i < series))


class RacingBarChart(Scene):
    def place(self, part, clock, positions, weights):
        self.add(part)
        self.bind_sampled(part, clock, "position",
                          [(i, *screen(x, y)) for i, (x, y) in enumerate(positions)],
                          linear=False)
        self.bind_sampled(part, clock, "opacity", weights, linear=False)

    def cap(self, color, start):
        return Polygon(*[(BAR_RADIUS * cos(start + i * pi / 24) / PIXELS_PER_UNIT,
                          BAR_RADIUS * sin(start + i * pi / 24) / PIXELS_PER_UNIT, 0)
                         for i in range(25)],
                       color=color, fill_opacity=1, stroke_opacity=0)

    def construct(self):
        self.add(Rectangle(width=16, height=9, fill="#0b1f3a", fill_opacity=1, stroke=None))
        values = [current_values(frame) for frame in range(FRAME_COUNT)]
        clock = self.value_tracker(0)
        title = MarkupText("<b>Market Share Race</b>",
                           font_size=34, color=WHITE)
        title.stretch(600 / (title.width * PIXELS_PER_UNIT), 0)
        title.move_to((*screen(83 + title.width * PIXELS_PER_UNIT / 2, 116), 0))
        self.add(title)
        self.bind_sampled(title, clock, "opacity",
                          [(frame, 1 - (1 - frame / 12) ** 3) for frame in range(13)],
                          linear=False)
        weights = [(frame, max(0, min(1, (frame-18)/8))) for frame in range(27)]
        for index, (name, _, color) in enumerate(DATA):
            y = [y_at(row_rank(row, index)) for row in values]
            widths = [max(60, BAR_AVAILABLE * row[index] / max(row)) for row in values]
            label = MarkupText("<b>" + name + "</b>",
                               font_size=8.5, color=WHITE)
            self.place(label, clock, [(180 - label.width * PIXELS_PER_UNIT/2, h) for h in y], weights)
            left = self.cap(color, pi / 2)
            right = self.cap(color, -pi / 2)
            body = Rectangle(width=1, height=48 / PIXELS_PER_UNIT,
                             fill=color, fill_opacity=1, stroke=None)
            self.place(left, clock, [(BAR_LEFT + BAR_RADIUS, h) for h in y], weights)
            self.place(right, clock, [(BAR_LEFT + w - BAR_RADIUS, h) for w, h in zip(widths, y)], weights)
            self.place(body, clock, [(BAR_LEFT + w / 2, h) for w, h in zip(widths, y)], weights)
            self.bind_sampled(body, clock, "scale",
                              [(i, (w - 2 * BAR_RADIUS + 0.2) / PIXELS_PER_UNIT, 1) for i, w in enumerate(widths)],
                              linear=False)

            rounded = [int(row[index] + 0.5) for row in values]
            for number in sorted(set(rounded)):
                digits = MarkupText("<b>" + str(number) + "</b>",
                                    font_size=8.5, color=WHITE)
                self.place(digits, clock,
                           [(BAR_LEFT + w - 16 - digits.width * PIXELS_PER_UNIT/2, h)
                            for w, h in zip(widths, y)],
                           [(i, max(0, min(1, (i-18)/8)) * int(value == number))
                            for i, value in enumerate(rounded)])
        self.play(clock.animate.set_value(18), run_time=18 / FPS, rate_func=linear)
        self.play(clock.animate.set_value(162), run_time=144 / FPS, rate_func=linear)
        self.wait(48 / FPS)
