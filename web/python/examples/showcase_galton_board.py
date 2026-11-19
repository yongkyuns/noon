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


def peg(row: int, k: int) -> np.ndarray:
    """The k-th peg (from the left) of a row."""
    return np.array([(k - row / 2) * DX, TOP - row * DY, 0.0])


def pile(count: int) -> np.ndarray:
    """Where the balls of a bin come to rest, relative to its floor's middle: rows of five
    and of four in turn, packed, each row filled in a scattered order."""
    fives = np.array([0.0, -2.0, 2.0, -1.0, 1.0]) * GAP
    fours = np.array([-0.5, 1.5, 0.5, -1.5]) * GAP
    places = np.empty((count, 2))
    for s in range(count):
        pair, r = divmod(s, 9)
        layer, x = (2 * pair, fives[r]) if r < 5 else (2 * pair + 1, fours[r - 5])
        places[s] = [x, BALL + layer * GAP * np.sqrt(3) / 2]
    return places


def release_times() -> np.ndarray:
    """When each ball is let go, in board time: three alone, then faster and faster."""
    first = [0.0, 1.4, 2.8]
    times, s = list(first), 0.0
    while len(times) < BALLS:
        times.append(4.2 + s)
        s += 1 / min(2 + 9 * s**2, 70)  # balls per second, s seconds into the rush
    return np.array(times)


class Board:
    """Every ball's way down, worked out in advance: its release, its bounces, its rest."""

    def __init__(self, releases: np.ndarray, rng: np.random.Generator) -> None:
        count = len(releases)
        self.releases = releases
        rights = rng.integers(0, 2, size=(count, ROWS))  # 1: it bounced right
        self.bins = rights.sum(1)
        before = np.concatenate([np.zeros((count, 1), int), np.cumsum(rights, 1)], 1)
        rows = np.arange(ROWS)
        # where it touches the peg of each row: above its center, by the two radii
        self.contacts = np.stack(
            [
                (before[:, :ROWS] - rows / 2) * DX,
                np.broadcast_to(TOP - rows * DY + PEG + BALL, (count, ROWS)),
            ],
            -1,
        )
        self.fall = float(np.sqrt(2 * (RELEASE - self.contacts[0, 0, 1]) / GRAVITY))
        self.kick = (
            GRAVITY * HOP**2 / 2 - DY
        ) / HOP  # up, off a peg: each hop drops DY
        # it comes to rest in its bin on top of those that came before it
        rank = np.zeros(count, int)
        self.counts = np.zeros(ROWS + 1, int)
        for i in np.argsort(releases, kind="stable"):
            rank[i] = self.counts[self.bins[i]]
            self.counts[self.bins[i]] += 1
        places = pile(int(self.counts.max()))
        self.rest = places[rank] + np.column_stack(
            [(self.bins - ROWS / 2) * DX, np.full(count, FLOOR)]
        )
        drop = self.contacts[:, -1, 1] - self.rest[:, 1]
        self.last = (self.kick + np.sqrt(self.kick**2 + 2 * GRAVITY * drop)) / GRAVITY
        self.landed = releases + self.fall + (ROWS - 1) * HOP + self.last

    def flight(self, i: int) -> np.ndarray:
        """One bead's time and position per retained keyframe."""
        start = self.releases[i]
        contact = self.contacts[i]
        rest = self.rest[i]
        path = [(start, 0.0, RELEASE)]
        segments = [(np.array([0.0, RELEASE]), contact[0], self.fall, 0.0)]
        segments.extend((a, b, HOP, self.kick) for a, b in zip(contact, contact[1:]))
        segments.append((contact[-1], rest, self.last[i], self.kick))
        for a, b, duration, speed in segments:
            samples = int(np.ceil(120 * duration))
            for k in range(1, samples + 1):
                u = duration * k / samples
                x = a[0] + (b[0] - a[0]) * k / samples
                y = b[1] if k == samples else a[1] + speed * u - GRAVITY * u*u / 2
                path.append((start + u, x, y))
            start += duration
        return np.array(path)


class GaltonBoard(Scene):
    def construct(self) -> None:
        board = Board(release_times(), np.random.default_rng(SEED))

        pegs = VGroup(
            *(
                Dot(peg(r, k), radius=PEG, color=GREY_B)
                for r in range(ROWS)
                for k in range(r + 1)
            )
        )
        wall_top = TOP - (ROWS - 1) * DY - 0.45
        edges = (np.arange(ROWS + 2) - (ROWS + 1) / 2) * DX
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
        end = float(board.landed.max()) + 0.2
        film = np.linspace(0, 30, 3001)
        speed = np.interp(film, [0, 5.0, 8.0], [0.8, 0.8, 2.0])
        board_time = np.concatenate(
            [[0], np.cumsum((speed[1:] + speed[:-1]) / 2 * np.diff(film))]
        )
        seconds = float(np.interp(end, board_time, film))
        clock = self.value_tracker(0)

        self.next_section("Build the board")
        balls = [Dot(radius=BALL, color=BLUE) for _ in board.releases]
        self.add(*balls)
        for i, ball in enumerate(balls):
            flight = board.flight(i)
            flight[:, 0] = np.interp(flight[:, 0], board_time, film)
            self.bind_sampled(ball, clock, "position", flight)
            self.bind_sampled(ball, clock, "opacity", [(-1, 0), (flight[0, 0], 1)], linear=False)

        # Keep the small integer repertoire resident. The counter changes at
        # the exact same landing times as the original scene.
        count_events = np.interp(np.sort(board.landed), board_time, film)
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
        mean, variance = ROWS / 2, ROWS / 4
        per_ball = GAP * np.sqrt(3) / 9  # a pile's height, per ball in it

        def bell(x: float) -> float:
            k = x / DX + ROWS / 2  # x, in bins
            density = np.exp(-((k - mean) ** 2) / (2 * variance)) / np.sqrt(
                2 * np.pi * variance
            )
            return FLOOR + BALLS * density * per_ball

        curve = FunctionGraph(bell, x_range=[edges[0], edges[-1]], color=YELLOW)
        curve.set_stroke(width=6)
        label = VGroup(
            MathTex(rf"\mu = {mean:g}", font_size=52),
            MathTex(rf"\sigma^2 = {variance:g}", font_size=52),
        ).arrange(DOWN, aligned_edge=LEFT, buff=0.25)
        label.set_color(YELLOW).move_to(np.array([edges[-1] + 1.6, FLOOR + 2.2, 0.0]))
        self.play(Create(curve), FadeIn(label), run_time=2.0)
        self.wait(2.0)
