"""Noon lesson inspired by Academa's ManimGX Galton board and 3Blue1Brown's CLT lesson.

https://manimgx.academa.ai/gallery/galton-board/
https://www.3blue1brown.com/lessons/clt/
This demonstration models independent fair turns at idealized pegs.
"""
from noon import *


def route(bits):
    x, y = -3.4, 2.18
    points = [(x, y, 0)]
    for bit in bits:
        points.append((x, y - 0.25, 0))
        x += 0.58 if bit else -0.58
        y -= 0.95
        points.append((x, y, 0))
    return points


def path(points, color, width):
    return VMobject(stroke_color=color, stroke_width=width).set_points_as_corners(points)


def label(text, x, y, size=24, color=WHITE):
    return Text(text, font_size=size, color=color).move_to((x, y, 0))


def histogram(slot, fraction):
    height = max(0.02, 5.3 * fraction)
    return Rectangle(width=0.64, height=1, fill_color=TEAL, fill_opacity=0.78,
                     stroke_width=0).stretch_to_fit_height(height).move_to((slot, -1.25 + height / 2, 0))


class GaltonBoard(Scene):
    def construct(self):
        self.next_section("One bead")
        heading = label("Why do beads pile up in the middle?", 0, 3.18, 41)
        caption = label("Each peg gives the bead two equally likely turns.", 0, -3.05, 26, GREY_B)
        bins = [-5.14 + 1.16 * k for k in range(4)]
        board = []
        for row in range(3):
            for k in range(row + 1):
                board.append(Dot((-3.4 + (2 * k - row) * 0.58, 1.93 - row * 0.95),
                                 radius=0.065, color=GREY_B))
        for x in bins:
            board.append(Line((x - 0.42, -0.87), (x + 0.42, -0.87),
                              color=BLUE_D, stroke_width=3))
        self.play(Write(heading), FadeIn(VGroup(*board)), run_time=1.0)
        self.play(FadeIn(caption), run_time=0.5)
        points = route((0, 1, 1))
        trail, bead = path(points, GOLD, 3), Dot(points[0], radius=0.095, color=YELLOW)
        track = trail.copy().set_object_opacity(0)
        self.add(track)
        self.play(FadeIn(bead), run_time=0.25)
        self.play(MoveAlongPath(bead, track), Create(trail), run_time=2.1, rate_func=linear)
        self.remove(track)
        self.wait(0.7)

        self.next_section("Count routes")
        routes = (("LLL",), ("LLR", "LRL", "RLL"), ("LRR", "RLR", "RRL"), ("RRR",))
        cards, counts = [], []
        slots = [0.85 + 1.45 * k for k in range(4)]
        for x, slot, words in zip(bins, slots, routes):
            counts.append(label(str(len(words)), x, -1.24, 29, GOLD))
            for i, word in enumerate(words):
                cards.append(label(word, slot, 1.30 - i * 0.49, 24,
                                   GOLD if word == "LRR" else GREY_B))
            cards.append(label(f"{len(words)} / 8", slot, -0.47, 26, GOLD))
        count_caption = label("Different routes can land in the same bin.", 0, -3.05, 26, GREY_B)
        route_heading = label("Eight possible routes", 3.03, 2.25, 29)
        self.play(FadeOut(caption), FadeIn(count_caption), Write(route_heading),
                  run_time=0.65)
        route_labels = cards.copy()
        for k, words in enumerate(routes):
            self.play(FadeIn(counts[k]), *(FadeIn(card) for card in cards[:len(words) + 1]),
                      run_time=0.55)
            cards = cards[len(words) + 1:]
        self.wait(1.0)

        self.next_section("Compare with many drops")
        self.play(FadeOut(trail), FadeOut(bead), FadeOut(count_caption), run_time=0.5)
        axis = Line((0.4, -1.25), (5.7, -1.25), color=GREY_B, stroke_width=2)
        expected = []
        bars = []
        for slot, words in zip(slots, routes):
            y = -1.25 + 5.3 * len(words) / 8
            expected.append(Line((slot - 0.40, y), (slot + 0.40, y), color=GOLD, stroke_width=3))
            bars.append(histogram(slot, 0))
        # Keep the route count below the chart, leaving the measured heights clear.
        self.play(FadeOut(route_heading), *(FadeOut(card) for card in route_labels),
                  run_time=0.45)
        # Noon's retained path motion uses a path attached to the scene. Reuse
        # eight invisible tracks for all the sample beads.
        tracks = {}
        for words in routes:
            for word in words:
                bits = tuple(letter == "R" for letter in word)
                tracks[bits] = path(route(bits), TEAL, 2).set_object_opacity(0)
        self.add(*bars, *tracks.values())
        self.play(Create(axis), *(Create(mark) for mark in expected),
                  *(FadeIn(label(f"{k} right", slot, -1.61, 18, GREY_B))
                    for k, slot in enumerate(slots)),
                  FadeIn(label("Gold: predicted share    Mint: sample share", 3.03, -2.15, 18)),
                  run_time=0.6)
        tally = [0] * 4
        total = 0
        seed = 1729
        sample = Variable(0, "beads", num_decimal_places=0, font_size=26)
        sample.move_to((-3.4, -1.95, 0))
        self.add(sample)
        self.wait(0.25)
        for _ in range(4):
            drops, motions = [], []
            for _ in range(12):
                seed = (1664525 * seed + 1013904223) % (2 ** 32)
                bits = tuple((seed >> (31 - row)) & 1 for row in range(3))
                total += 1
                tally[sum(bits)] += 1
                points = route(bits)
                dot = Dot(points[0], radius=0.064, color=TEAL)
                drops.append(dot)
                motions.append(MoveAlongPath(dot, tracks[bits], rate_func=linear))
            self.add(*drops)
            self.play(LaggedStart(*motions, lag_ratio=0.06), run_time=1.65, rate_func=linear)
            self.remove(*drops)
            self.play(*(Transform(bar, histogram(slot, n / total))
                        for bar, slot, n in zip(bars, slots, tally)),
                      sample.tracker.animate.set_value(total),
                      run_time=0.4)
        self.play(Write(label("More routes lead inward, so the middle bins are more likely.",
                              0, -3.05, 26, GREY_B)), run_time=0.8)
        self.wait(2.0)
