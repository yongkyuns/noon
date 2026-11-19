# Copyright (c) 2026 Academa, Inc.
# SPDX-License-Identifier: MIT
# See THIRD_PARTY_NOTICES.md for license and provenance.
# Source: https://manimgx.academa.ai/gallery/galton-board/
# https://github.com/academa-labs/manimgx/blob/1fb4dd7327bf5ed7d0c40d022c05cb816455ae1d/examples/galton_board.py
# The seeded board, release pace and storyboard are from ManimGX. Noon samples
# each ballistic segment into the existing Rust timeline before playing it.
"""Randomness that adds up to a bell: the Galton board.

Every ball meets a peg in each of the board's rows and bounces left or right, at random, as
if by a coin toss. The bin it lands in counts how many times it went right, so a ball's bin is
a sum of coin tosses, and the number of balls in bin k follows the binomial distribution: of
the 2ⁿ ways down, C(n, k) end in bin k. A few balls make a ragged pile; many make a bell, the
normal curve with the binomial's mean n/2 and variance n/4 (de Moivre, 1733; Galton, 1889).
"""

from bisect import bisect_right
from math import exp, pi, sqrt, ceil

import numpy as np

from noon import *

ROWS = 10  # rows of pegs; there are ROWS + 1 bins
BALLS = 480
DX, DY = 0.7, 0.34  # between pegs in a row, and between rows
TOP = 3.1  # the first peg's height
FLOOR = -3.75  # the bins' floor
PEG, BALL = 0.05, 0.06  # radii
GAP = 2.04 * BALL  # between the centers of two balls side by side in a pile
GRAVITY = 25.0  # scene units per second², in the board's own time
HOP = 0.26  # seconds from one peg to the next, in the board's own time
RELEASE = TOP + 0.62  # where balls are let go, in the funnel's neck
SEED = 7


def peg(row, k):
    """The k-th peg (from the left) of a row."""
    return (k - row / 2) * DX, TOP - row * DY, 0.0


def pile_slot(rank):
    """Rows of five and four offset balls, packed within one bin."""
    fives = (0.0, -2.0, 2.0, -1.0, 1.0)
    fours = (-0.5, 1.5, 0.5, -1.5)
    pair, rest = divmod(rank, 9)
    layer, x = (2 * pair, fives[rest]) if rest < 5 else (2 * pair + 1, fours[rest - 5])
    return x * GAP, BALL + layer * GAP * sqrt(3) / 2


def release_times():
    """Three balls alone, then a progressively faster stream."""
    times, s = [0.0, 1.4, 2.8], 0.0
    while len(times) < BALLS:
        times.append(4.2 + s)
        s += 1 / min(2 + 9 * s**2, 70)
    return times


def contacts_for_ball(choices):
    contacts = []
    rights = 0
    for row in range(ROWS):
        contacts.append(((rights - row / 2) * DX, TOP - row * DY + PEG + BALL))
        rights += int(choices[row])
    return contacts, rights


def flight(release, contacts, rest, fall, kick, final_drop):
    """One bead's time and position per retained keyframe."""
    path = [(release, 0.0, RELEASE)]
    segments = [((0.0, RELEASE), contacts[0], fall, 0.0)]
    segments.extend((a, b, HOP, kick) for a, b in zip(contacts, contacts[1:]))
    segments.append((contacts[-1], rest, final_drop, kick))
    start = release
    for a, b, duration, speed in segments:
        samples = ceil(120 * duration)
        for k in range(1, samples + 1):
            u = duration * k / samples
            x = a[0] + (b[0] - a[0]) * k / samples
            y = b[1] if k == samples else a[1] + speed * u - GRAVITY * u*u / 2
            path.append((start + u, x, y))
        start += duration
    return path


def film_time(time, board_time, film):
    i = bisect_right(board_time, time) - 1
    if i < 0:
        return film[0]
    if i == len(board_time) - 1:
        return film[-1]
    slope = (film[i + 1] - film[i]) / (board_time[i + 1] - board_time[i])
    return film[i] + slope * (time - board_time[i])


class GaltonBoard(Scene):
    def construct(self):
        releases = release_times()
        choices = np.random.default_rng(SEED).integers(0, 2, size=(BALLS, ROWS))

        pegs = VGroup(
            *(
                Dot(peg(r, k), radius=PEG, color=GREY_B)
                for r in range(ROWS)
                for k in range(r + 1)
            )
        )
        wall_top = TOP - (ROWS - 1) * DY - 0.45
        edges = [(k - (ROWS + 1) / 2) * DX for k in range(ROWS + 2)]
        bins = VGroup(
            Line([edges[0], FLOOR, 0], [edges[-1], FLOOR, 0]),
            *(Line([x, FLOOR, 0], [x, wall_top, 0]) for x in edges),
        ).set_stroke(GREY_B, width=3)

        funnel = VGroup(
            Line([-0.5, RELEASE + 0.22, 0], [-0.1, RELEASE - 0.05, 0]),
            Line([0.5, RELEASE + 0.22, 0], [0.1, RELEASE - 0.05, 0]),
        ).set_stroke(GREY_B, width=3)

        # The source runs one board clock against the film clock. Use the inverse
        # of that curve for the retained keys, so releases and landings keep their timing.
        film = [k * 0.01 for k in range(3001)]
        board_time = [0.0]
        speed = 0.8
        elapsed = 0.0
        for previous, t in zip(film, film[1:]):
            next_speed = 0.8 + (2 - 0.8) / 3 * min(3, max(0, t - 5))
            elapsed += (speed + next_speed) / 2 * (t - previous)
            board_time.append(elapsed)
            speed = next_speed
        clock = self.value_tracker(0)

        self.next_section("Build the board")
        balls = [Dot(radius=BALL, color=BLUE) for release in releases]
        self.add(*balls)
        counts = [0 for k in range(ROWS + 1)]
        landed = []
        fall = sqrt(2 * (RELEASE - (TOP + PEG + BALL)) / GRAVITY)
        kick = (GRAVITY * HOP**2 / 2 - DY) / HOP
        for i, ball in enumerate(balls):
            contacts, bin_index = contacts_for_ball(choices[i])
            x, y = pile_slot(counts[bin_index])
            counts[bin_index] += 1
            rest = x + (bin_index - ROWS / 2) * DX, y + FLOOR
            drop = contacts[-1][1] - rest[1]
            final_drop = (kick + sqrt(kick**2 + 2 * GRAVITY * drop)) / GRAVITY
            path = flight(releases[i], contacts, rest, fall, kick, final_drop)
            keys = [(film_time(t, board_time, film), x, y) for t, x, y in path]
            self.bind_sampled(ball, clock, "position", keys)
            self.bind_sampled(ball, clock, "opacity", [(-1, 0), (keys[0][0], 1)], linear=False)
            landed.append(releases[i] + fall + (ROWS - 1) * HOP + final_drop)
        seconds = round(film_time(max(landed) + 0.2, board_time, film), 9)

        # Keep the small integer repertoire resident. The counter changes at
        # the exact same landing times as the original scene.
        count_events = [film_time(time, board_time, film) for time in sorted(landed)]
        word = Tex("balls", font_size=44)
        zero = Integer(0, font_size=56)
        VGroup(word, zero).arrange(RIGHT, buff=0.3).to_corner(UL)
        digit_width = 1.1 * zero.width
        for place in range(3):
            for digit in range(10):
                glyph = Integer(digit, font_size=56).move_to(zero.get_center() + place * digit_width * RIGHT)
                self.add(glyph)
                # Hundreds and tens stay absent until a large enough value.
                previous = int(place == 0 and digit == 0)
                keys = [(-1.0, previous)]
                for total, time in enumerate(count_events, 1):
                    text = str(total)
                    visible = int(place < len(text) and int(text[place]) == digit)
                    if visible != previous:
                        keys.append((time, visible))
                        previous = visible
                self.bind_sampled(glyph, clock, "opacity", keys, linear=False)

        self.play(
            LaggedStart(*(FadeIn(p, scale=0.5) for p in pegs), lag_ratio=0.02),
            Create(bins),
            Create(funnel),
            FadeIn(word),
            run_time=1.2,
        )
        self.next_section("Drop the balls")
        self.play(clock.animate.set_value(seconds), run_time=seconds, rate_func=linear)
        self.next_section("Fit the normal curve")

        # the bell: the normal curve with the binomial's mean and variance, over the bins
        mean = ROWS / 2
        variance = ROWS / 4
        per_ball = GAP * sqrt(3) / 9  # a pile's height, per ball in it

        def bell(x):
            k = x / DX + ROWS / 2  # x, in bins
            density = exp(-((k - mean) ** 2) / (2 * variance)) / sqrt(2 * pi * variance)
            return FLOOR + BALLS * density * per_ball

        curve = FunctionGraph(bell, x_range=[edges[0], edges[-1]], color=YELLOW)
        curve.set_stroke(width=6)
        label = VGroup(
            MathTex(rf"\mu = {mean:g}", font_size=52),
            MathTex(rf"\sigma^2 = {variance:g}", font_size=52),
        ).arrange(DOWN, aligned_edge=LEFT, buff=0.25)
        label.set_color(YELLOW).move_to((edges[-1] + 1.6, FLOOR + 2.2, 0.0))
        self.play(Create(curve), FadeIn(label), run_time=2.0)
        self.wait(2.0)
