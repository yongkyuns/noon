# Copyright (c) 2026 Academa, Inc.
# SPDX-License-Identifier: MIT
# See THIRD_PARTY_NOTICES.md for license and provenance.
# Source: https://manimgx.academa.ai/gallery/fourier-pi/
# https://github.com/academa-labs/manimgx/blob/1fb4dd7327bf5ed7d0c40d022c05cb816455ae1d/examples/fourier_pi.py
# The upstream scene credits Grant Sanderson's Fourier series lesson:
# https://www.3blue1brown.com/lessons/fourier-series
"""The original π trace, zoom, and equation with native retained pose sampling."""

import numpy as np

from noon import *


TERMS = 50
PERIOD = 10.0
SAMPLES = 4096
DRAW_START = 4.5
DRAW_END = DRAW_START + PERIOD
FOLLOW_END = DRAW_END + 7
FINISH = FOLLOW_END + 2

# Italic π outline from New Computer Modern Math Regular (GUST Font License).
# Extracted from the bundled Typst font at U+1D70B. Noon uses these same
# vertices for the Fourier coefficients and the vector reference. This
# outline stays sharp through the pen-follow zoom.
PI_CONTOUR = """M 567 407
C 567 431 546 431 527 431
L 192 431
C 170 431 132 431 88 384
C 53 345 27 299 27 294
C 27 294 27 284 39 284
C 47 284 49 288 55 296
C 104 373 162 373 182 373
L 239 373
C 207 252 153 131 111 40
C 103 25 103 23 103 16
C 103 -3 119 -11 132 -11
C 162 -11 170 17 182 54
C 196 100 196 102 209 152
L 265 373
L 378 373
C 345 225 336 182 336 115
C 336 100 336 73 344 39
C 354 -5 365 -11 380 -11
C 400 -11 421 7 421 27
C 421 33 421 35 415 49
C 386 121 386 186 386 214
C 386 267 393 321 404 373
L 518 373
C 531 373 567 373 567 407
Z"""


def glyph_curves():
    curves = []
    for command in PI_CONTOUR.splitlines():
        op, *values = command.split()
        points = np.array(values, dtype=float).reshape(-1, 2)
        if op == "M":
            start = cursor = points[0]
            continue
        if op == "C":
            b, c, end = points
        else:
            end = points[0] if op == "L" else start
            b = (2*cursor + end) / 3
            c = (cursor + 2*end) / 3
        curves.append((cursor, b, c, end))
        cursor = end
    return (np.array(curves) - [297.0, 210.0]) * (5.0 / 442.0) + [0.0, 0.55]


def outline(curves):
    dense = []
    t = np.linspace(0, 1, 64, endpoint=False)[:, None]
    for a, b, c, d in curves:
        dense.extend((1-t)**3*a + 3*(1-t)**2*t*b + 3*(1-t)*t*t*c + t**3*d)
    closed = np.array(dense + [dense[0]])
    run = np.concatenate([[0.0], np.cumsum(np.linalg.norm(np.diff(closed, axis=0), axis=1))])
    at = np.linspace(0.0, run[-1], SAMPLES, endpoint=False)
    return np.interp(at, run, closed[:, 0]) + 1j * np.interp(at, run, closed[:, 1])


def coefficients(z):
    spectrum = np.fft.fft(z) / len(z)
    frequencies = np.array([0] + [n for k in range(1, TERMS + 1) for n in (k, -k)])
    return frequencies, spectrum[frequencies % len(z)]


def smooth_edge(start, end, time):
    a = np.clip((time - start) / (end - start), 0.0, 1.0)
    # A native tracker runs linearly through these finite keys. Authored
    # event timing uses the gallery scene's 1.5-second zooms and four-second hold.
    return a * a * (3 - 2*a)


def positions(times, complex_points):
    return np.column_stack((times, complex_points.real, complex_points.imag))


class FourierPi(MovingCameraScene):
    def construct(self):
        curves = glyph_curves()
        frequencies, c = coefficients(outline(curves))
        # Sample camera and moving joints at 120 Hz. Every arrow's angle is
        # a separate linear, unwrapped key track, so fast terms never reverse.
        times = np.linspace(0.0, FINISH, int(FINISH * 120) + 1)
        lap = np.clip((times - DRAW_START) / PERIOD, 0.0, 1.7)
        angles = TAU * lap[:, None] * frequencies
        joints = np.cumsum(c * np.exp(1j * angles), axis=1)
        clock = self.value_tracker(0)
        pen_path = joints[:, -1]
        close = smooth_edge(DRAW_END, DRAW_END + 1.5, times) * (
            1 - smooth_edge(FOLLOW_END - 1.5, FOLLOW_END, times))
        camera_scale = 1 - 0.78*close
        frame = self.camera.frame
        self.bind_sampled(frame, clock, "position", positions(times, close * pen_path))
        self.bind_sampled(frame, clock, "scale",
                          np.column_stack((times, camera_scale, camera_scale)))

        pen = Dot(radius=0.045, color=YELLOW)
        self.add(pen)
        self.bind_sampled(pen, clock, "position", positions(times, pen_path))
        self.bind_sampled(pen, clock, "scale",
                          np.column_stack((times, camera_scale, camera_scale)))
        hide = 1 - smooth_edge(FOLLOW_END, FINISH, times)
        self.bind_sampled(pen, clock, "opacity",
                          np.column_stack((times, (times >= DRAW_START) * hide)),
                          linear=False)

        for i, coefficient in enumerate(c[1:]):
            radius = float(abs(coefficient))
            circle = Circle(radius=radius, stroke_width=1.5, stroke_opacity=0.6, color=BLUE,
                            stroke_width_mode="screen_space")
            arrow = Arrow(ORIGIN, radius * RIGHT, buff=0, stroke_width=2.5,
                          tip_length=0.2, max_tip_length_to_length_ratio=0.25,
                          max_stroke_width_to_length_ratio=12, color=GREY_A,
                          stroke_width_mode="screen_space")
            self.add(circle, arrow)
            origin = joints[:, i]
            self.bind_sampled(circle, clock, "position", positions(times, origin))
            self.bind_sampled(circle, clock, "stroke_width", np.column_stack((times, 1.5*camera_scale)))
            self.bind_sampled(circle, clock, "opacity",
                              np.column_stack((times, smooth_edge(2.5, DRAW_START, times) * hide)))
            turn = angles[:, i+1] + np.angle(coefficient)
            if i < 24:
                step = 2 / (1 + 23 * 0.12)
                grow = smooth_edge(2.5 + 0.12 * i * step, 2.5 + (1 + 0.12 * i) * step, times)
            else:
                grow = np.ones_like(times)
            for part_index, part in enumerate(arrow):
                self.bind_sampled(part, clock, "position", positions(times, origin))
                if part_index == 0:
                    widths = min(2.5, 12*radius) * camera_scale
                    self.bind_sampled(part, clock, "stroke_width", np.column_stack((times, widths)))
                self.bind_sampled(part, clock, "rotation",
                                  [(0.0, turn[0]), (DRAW_START, turn[0]),
                                   (FOLLOW_END, turn[-1]), (FINISH, turn[-1])])
                if i < 24:
                    # Clamp the scale away from zero for stable path bounds.
                    scale = np.maximum(grow, 0.0001)
                    self.bind_sampled(part, clock, "scale", np.column_stack((times, scale, scale)))
                    opacity = (grow > 0) * hide
                else:
                    opacity = smooth_edge(2.5, DRAW_START, times) * hide
                self.bind_sampled(part, clock, "opacity", np.column_stack((times, opacity)))

        phases = np.linspace(0.0, 1.0, 2400)
        curve = np.exp(1j * TAU * np.outer(phases, frequencies)) @ c
        drawn = VMobject(stroke_color=YELLOW, stroke_width=5, stroke_width_mode="screen_space")
        drawn.set_points_as_corners(np.column_stack((curve.real, curve.imag, np.zeros_like(phases))))
        drawn.set_z_index(-1)
        filled = drawn.copy().set_fill(YELLOW, opacity=1)
        traced = drawn.copy()
        self.add(traced)
        self.bind_sampled(traced, clock, "opacity", [(0, 0), (DRAW_END-0.0001, 0), (DRAW_END, 1)])
        self.bind_sampled(traced, clock, "stroke_width", np.column_stack((times, 5*camera_scale)))
        glyph_xy = curves.reshape(-1, 2)
        glyph_points = np.column_stack((glyph_xy, np.zeros(len(glyph_xy))))
        letter = VMobject(fill_color=WHITE, fill_opacity=1, stroke_width=0)
        letter.set_points(glyph_points).set_z_index(-2)
        ghost = letter.copy()
        self.add(ghost)
        self.bind_sampled(ghost, clock, "opacity",
                          [(0, 0), (1.4999, 0), (1.5, 1), (2.5, 0.18),
                           (FOLLOW_END, 0.18), (FINISH, 0)])
        formula = MathTex(r"z(t) = \sum_{n=-50}^{50} c_n\, e^{2\pi i n t}", font_size=48)
        formula.to_edge(DOWN, buff=0.3)

        self.next_section("Reveal the target")
        self.play(Create(letter), clock.animate.set_value(1.5), run_time=1.5, rate_func=linear)
        self.remove(letter)
        self.play(clock.animate.set_value(2.5), run_time=1.0, rate_func=linear)
        self.play(clock.animate.set_value(DRAW_START), run_time=2.0, rate_func=linear)

        self.next_section("Trace one revolution")
        self.play(Create(drawn), clock.animate.set_value(DRAW_END), run_time=PERIOD, rate_func=linear)
        self.remove(drawn)
        self.next_section("Follow the pen")
        self.play(clock.animate.set_value(FOLLOW_END), run_time=7.0, rate_func=linear)

        self.next_section("The Fourier series")
        self.play(FadeIn(filled), Write(formula),
                  clock.animate.set_value(FINISH), run_time=2.0, rate_func=linear)
        self.wait(2.0)
