# Copyright (c) 2022 motion-canvas
# SPDX-License-Identifier: MIT
# Original: https://motioncanvas.io/docs/signals/#complex-example
# https://github.com/motion-canvas/motion-canvas/blob/7b91435c301d530351dcf5ebb91dd139c002e405/packages/examples/src/scenes/node-signal.tsx
# See THIRD_PARTY_NOTICES.md.
"""One radius drives the disk, its dimension line, and the area readout."""

from math import pi

from noon import *


FPS = 30
PIXELS_PER_UNIT = 135
RED_FILL = "#e13238"
INK = "#242424"
# Advances of the source's 56-pixel bold browser serif face.
RADIUS_ADVANCES = (21.8, 14, 31.9, 14, 28, 14, 28, 28)
AREA_ADVANCES = (40.4, 14, 31.9, 14, 28, 28, 14, 28, 28)


def point(x, y):
    return (x / PIXELS_PER_UNIT, -y / PIXELS_PER_UNIT, 0)


def cubic(t):
    return 4 * t ** 3 if t < 0.5 else 1 - 4 * (1 - t) ** 3


def radius_at(frame):
    if frame < 60:
        return 3 + cubic(frame / 60)
    return 4 - cubic(min(1, (frame - 60) / 60))


def ink_box(width, height):
    return Rectangle(width=width / PIXELS_PER_UNIT, height=height / PIXELS_PER_UNIT,
                     fill=INK, fill_opacity=1, stroke=None)


class CircleAreaSignals(Scene):
    def readout(self, clock, values, locations, advances, color):
        # All readouts have a fixed format. Reuse each glyph at its position
        # and select it with the same authored clock as the disk.
        line_width = sum(advances)
        for column, advance in enumerate(advances):
            offset = sum(advances[:column]) + (advance - line_width) / 2
            for char in sorted(set(value[column] for value in values)):
                if char == " ":
                    continue
                glyph = MarkupText("<b>" + char + "</b>", font="Libertinus Serif",
                                   font_size=28, color=color_from_hex(color))
                lift = glyph.height / 2
                if char == "=":
                    lift += 7 / PIXELS_PER_UNIT
                self.add(glyph)
                self.bind_sampled(glyph, clock, "position",
                    [(i, (x + offset) / PIXELS_PER_UNIT, lift - baseline / PIXELS_PER_UNIT)
                     for i, (x, baseline) in enumerate(locations)])
                self.bind_sampled(glyph, clock, "opacity",
                    [(i, int(value[column] == char)) for i, value in enumerate(values)],
                    linear=False)

    def construct(self):
        radii = [radius_at(frame) for frame in range(121)]
        clock = self.value_tracker(0)
        self.add(Rectangle(width=16, height=9, fill=WHITE, fill_opacity=1, stroke=None))
        circle = Circle(radius=300 / PIXELS_PER_UNIT, fill=RED_FILL,
                        fill_opacity=1, stroke=None)
        self.add(circle)
        self.bind_sampled(circle, clock, "scale",
                          [(i, r / 3, r / 3) for i, r in enumerate(radii)])

        # Motion Canvas starts the dashed stroke 12 pixels after the start
        # arrow and stops 20 pixels before the radius endpoint.
        for x in range(12, 380, 40):
            lengths = [max(0, min(20, 100 * r - 20 - x)) for r in radii]
            dash = ink_box(20, 8)
            self.add(dash)
            self.bind_sampled(dash, clock, "scale",
                              [(i, max(0.0001, length / 20), 1)
                               for i, length in enumerate(lengths)])
            self.bind_sampled(dash, clock, "position",
                              [(i, (x + length / 2) / PIXELS_PER_UNIT, 0)
                               for i, length in enumerate(lengths)])
            self.bind_sampled(dash, clock, "opacity",
                              [(i, int(length > 0)) for i, length in enumerate(lengths)],
                              linear=False)
        head = Polygon(point(0, 0), point(24, -24), point(24, 24),
                       color=INK, fill_opacity=1, stroke_opacity=0)
        tail = Polygon(point(0, 0), point(-24, -24), point(-24, 24),
                       color=INK, fill_opacity=1, stroke_opacity=0)
        self.add(head, tail)
        self.bind_sampled(tail, clock, "position",
                          [(i, (100 * r - 8) / PIXELS_PER_UNIT, 0)
                           for i, r in enumerate(radii)])
        self.readout(clock, [f"r = {r:.2f}" for r in radii],
                     [(50 * r, 72) for r in radii], RADIUS_ADVANCES, INK)
        self.readout(clock, [f"A = {pi*r*r:.2f}" for r in radii],
                     [(0, 100 * r + 72) for r in radii], AREA_ADVANCES, RED_FILL)

        self.play(clock.animate.set_value(2 * FPS), run_time=2, rate_func=linear)
        self.play(clock.animate.set_value(4 * FPS), run_time=2, rate_func=linear)
        self.wait(1)
