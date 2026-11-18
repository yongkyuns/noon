"""Noon reproduction of ManimGX's Rectangles become an integral (Academa).
Reference: https://manimgx.academa.ai/gallery/riemann-sums/
"""
from math import cos, sin

from noon import *

A = 0.5
B = 4.5  # Try 3.5 to change the upper limit.
REFINEMENTS = 6


def f(x):
    return 1 + 0.6 * x + 0.9 * sin(1.6 * x)


def antiderivative(x):
    return x + 0.3 * x * x - (0.9 / 1.6) * cos(1.6 * x)


def rectangle_sum(count):
    dx = (B - A) / count
    return dx * sum(f(A + i * dx) for i in range(count))


def left_at(mobject, x, y):
    return mobject.move_to((x + mobject.width / 2, y, 0))


def strip_color(fraction):
    return Color(*(a + (b - a) * fraction for a, b in zip(
        (BLUE.red, BLUE.green, BLUE.blue),
        (GREEN.red, GREEN.green, GREEN.blue),
    )))


def polygon(frame, points, color):
    x0, y0, x_unit, y_unit = frame
    shape = VMobject(fill_color=color, fill_opacity=1, stroke_width=0)
    shape.set_points_as_corners([(x0 + x * x_unit, y0 + y * y_unit, 0) for x, y in points])
    return shape.close_path()


def strips(frame, count, *, split=False, rise=1):
    dx = (B - A) / count
    heights = [rise * f(A + dx * (i - (i % 2 if split else 0))) for i in range(count)]
    if count > 32:
        # Join fine strips into staircases so their internal edges stay seamless.
        per_band = count // 32
        paths = []
        for band in range(32):
            first = band * per_band
            end = first + per_band
            left = A + first * dx
            # A subpixel overlap keeps neighboring bands joined.
            right = A + end * dx + (0.004 if end < count else 0)
            points = [(left, 0), (right, 0)]
            for i in reversed(range(first, end)):
                edge = right if i == end - 1 else A + (i + 1) * dx
                points.extend([(edge, heights[i]), (A + i * dx, heights[i])])
            paths.append(polygon(frame, points, strip_color((band + 0.5) / 32)))
        return paths

    def half_gap(width):
        x_unit = frame[2]
        return 0.03 * min(1, width * x_unit / 0.5) ** 2 / x_unit

    gap = half_gap(dx)
    parent_gap = half_gap(dx * 2)
    result = []
    for i, height in enumerate(heights):
        left_inset = (0 if i % 2 else parent_gap) if split else gap
        right_inset = (parent_gap if i % 2 else 0) if split else gap
        left = A + i * dx + left_inset
        right = A + (i + 1) * dx - right_inset
        midpoint = (i - i % 2 + 1) if split else (i + 0.5)
        result.append(polygon(frame, [(left, 0), (right, 0), (right, height), (left, height)],
                              strip_color(midpoint / count)))
    return result


def sum_formula():
    formula = left_at(MathTex(r"\sum_i", "f(", "x_i", ")", r"\,\Delta x", "=", font_size=54),
                      -0.91, 3.20)
    for term in ("f(", "x_i", ")"):
        formula.set_color_by_tex(term, YELLOW)
    return formula


class RiemannSums(Scene):
    class AreaReadout:
        """Schedule the formatted digits as retained opacity tracks."""

        def __init__(self, scene, amounts, durations, travel):
            self.cues = []
            visible = {}
            glyphs = set()
            for phase, duration in enumerate(durations):
                cues = {}
                frames = round(duration * 60)
                start, end = amounts[phase:phase + 2]
                for frame in range(1, frames + 1):
                    progress = smooth(frame / frames)
                    value = start + progress * (end - start)
                    opacity = progress if phase == 0 else 1
                    next_visible = self.digit_positions(value, opacity)
                    time = min(duration * frame / frames, duration - 0.005)
                    for key in sorted(visible.keys() | next_visible.keys()):
                        alpha = next_visible.get(key, 0)
                        if alpha != visible.get(key, 0):
                            cues.setdefault(key, []).append((time, alpha))
                    visible = next_visible
                self.cues.append((duration, cues))
                glyphs.update(cues)
            self.glyphs = {}
            for key in sorted(glyphs):
                slot, digit = key
                if digit == ".":
                    formula = MathTex(".", font_size=53)
                    scene.add(formula)
                    shape = formula[0]
                    y, advance = 3.22, 1
                else:
                    shape = DecimalNumber(int(digit), num_decimal_places=0, font_size=53)
                    scene.add(shape)
                    y, advance = 3.39, 2
                opacity = scene.value_tracker(0)
                x = 3.05 + 0.143 * (slot + advance / 2)
                scene.bind_position(shape, travel, direction=(-1.59, -0.16, 0), offset=(x, y, 0))
                scene.bind_opacity(shape, opacity)
                self.glyphs[key] = opacity

        @staticmethod
        def digit_positions(value, opacity):
            positions = {}
            slot = 0
            for digit in f"{value:.2f}":
                positions[(slot, digit)] = opacity
                slot += 1 if digit == "." else 2
            return positions

        def animate_phase(self, index):
            duration, cues = self.cues[index]
            tracks = []
            for key, changes in cues.items():
                pieces = []
                time = 0
                for end, opacity in changes:
                    # Finish before the sample so it shows the rounded value.
                    transition = min(0.004, end - time)
                    pause = end - time - transition
                    if pause > 0:
                        pieces.append(Wait(pause))
                    pieces.append(self.glyphs[key].animate(run_time=transition, rate_func=lingering).set_value(opacity))
                    time = end
                tracks.append(Succession(*pieces, rate_func=linear))
            return AnimationGroup(*tracks, Wait(duration), run_time=duration, rate_func=linear)

    def fade_track(self, *formulae):
        """Fade each TeX part through the retained opacity binding."""
        opacity = self.value_tracker(0)
        self.add(*formulae)
        for formula in formulae:
            for part in formula:
                self.bind_opacity(part, opacity)
        return opacity

    def width_label(self, dx, *, initial=False):
        number = left_at(MathTex(f"{dx:g}", font_size=49), -5.14, 3.30)
        opacity = self.fade_track(number)
        slide = self.value_tracker(1 if initial else 0)
        for part in number:
            x, y, z = part.get_center()
            self.bind_position(part, slide, direction=(0, 0.3, 0), offset=(x, y - 0.3, z))
        return opacity, slide

    def moving_term(self, tex, start, direction, opacity, travel):
        color = YELLOW if tex in ("f(", "x", "x_i", ")") else WHITE
        term = MathTex(tex, font_size=54, color=color)
        self.add(term)
        for part in term:
            self.bind_position(part, travel, direction=direction, offset=start)
            self.bind_opacity(part, opacity)

    def match_terms(self, summation, integral, travel):
        """Keep shared glyphs and move the changed terms to matching positions."""
        shared = self.value_tracker(0)
        outgoing = self.value_tracker(0)
        for before, after in zip(summation, integral):
            start = before.get_center()
            end = after.get_center()
            direction = tuple(b - a for a, b in zip(start, end))
            old, new = before.tex_string.strip(), after.tex_string.strip()
            self.moving_term(old, start, direction, shared if old == new else outgoing, travel)
            if old != new:
                self.moving_term(new, start, direction, travel, travel)
        return shared, outgoing

    def construct(self):
        if not 2 <= B <= 4.75:
            raise ValueError("Choose an upper limit from 2 to 4.75.")
        axes = Axes([0, 5, 1], [0, 5, 1], x_length=10.5, y_length=5.2,
                    tips=False, color=GRAY_B).move_to((0.2, -0.75, 0))
        curve = axes.plot(f, [0.05, 4.95, 0.025], color=YELLOW, stroke_width=6.7)
        curve.set_z_index(2)
        endpoint = axes.c2p(4.95, f(4.95))
        curve_name = left_at(MathTex("f(x)", color=YELLOW, font_size=48),
                             endpoint[0] + 0.2, endpoint[1])
        ends = [MathTex(name, font_size=44).move_to(axes.c2p(x, -0.32))
                for name, x in (("a", A), ("b", B))]
        gradient = LinearGradient([(0, BLUE), (1, GREEN)])

        width_eq = left_at(MathTex(r"\Delta x =", font_size=50), -6.6, 3.30)
        written_sum = sum_formula()
        integral = left_at(MathTex(r"\int_a^b", "f(", "x", ")", r"\,dx", "=", font_size=54),
                           -2.15, 3.19)
        for part in integral:
            if part.tex_string.strip() in ("f(", "x", ")"):
                part.set_color(YELLOW)
        truth = antiderivative(B) - antiderivative(A)
        goal = MathTex(rf"\to {truth:.2f}", font_size=51, color=GRAY_B).move_to((5.60, 3.39, 0))

        travel = self.value_tracker(0)
        self.add(axes)
        refinement_times = [1.8 if level <= 3 else 1.4 for level in range(1, REFINEMENTS + 1)]
        totals = [0] + [rectangle_sum(4 * 2**level) for level in range(REFINEMENTS + 1)] + [truth]
        readout = self.AreaReadout(self, totals, [1.5, *refinement_times, 2], travel)

        annotations = self.fade_track(curve_name, *ends)
        width_fade = self.fade_track(width_eq)
        goal_fade = self.fade_track(goal)
        widths = [self.width_label((B - A) / (4 * 2**level), initial=level == 0)
                  for level in range(REFINEMENTS + 1)]
        width_opacity, width_slide = widths[0]

        shared, outgoing = self.match_terms(written_sum, integral, travel)
        x0, y0 = axes.c2p(0, 0)[:2]
        x1, y1 = axes.c2p(1, 1)[:2]
        frame = (x0, y0, x1 - x0, y1 - y0)

        self.next_section("Trace the curve")
        self.play(Create(curve), annotations.animate.set_value(1), run_time=1.4)

        self.next_section("Grow the first four rectangles")
        active = strips(frame, 4, rise=0.002)
        self.add(*active)
        self.play(AnimationGroup(
            *(Transform(bar, target, run_time=1.5, rate_func=smooth)
              for bar, target in zip(active, strips(frame, 4))),
            width_fade.animate(run_time=1.5, rate_func=smooth).set_value(1),
            width_opacity.animate(run_time=1.5, rate_func=smooth).set_value(1),
            Write(written_sum, run_time=1.5, rate_func=smooth),
            readout.animate_phase(0),
            rate_func=linear,
        ))
        self.remove(written_sum)
        shared.set_value(1)
        outgoing.set_value(1)
        self.play(goal_fade.animate.set_value(1), run_time=0.5)

        self.next_section("Halve the width")
        for level, (next_opacity, next_slide) in enumerate(widths[1:], start=1):
            count = 4 * 2**level
            duration = refinement_times[level - 1]
            early = round(duration * 0.35, 2)
            halves = strips(frame, count, split=True)
            self.remove(*active)
            self.add(*halves)
            active = halves
            targets = strips(frame, count)
            self.play(AnimationGroup(
                *(Transform(bar, new, run_time=duration, rate_func=smooth)
                  for bar, new in zip(active, targets)),
                readout.animate_phase(level),
                width_opacity.animate(run_time=early, rate_func=smooth).set_value(0),
                width_slide.animate(run_time=early, rate_func=smooth).set_value(2),
                next_opacity.animate(run_time=early, rate_func=smooth).set_value(1),
                next_slide.animate(run_time=early, rate_func=smooth).set_value(1),
                lag_ratio=0,
            ))
            width_opacity, width_slide = next_opacity, next_slide
            self.wait(0.15)
        self.wait(0.4)

        self.next_section("Turn the sum into the integral")
        region = axes.get_area(curve, [A, B], color=BLUE, opacity=1, stroke_width=0)
        region.set_fill(gradient, opacity=1)
        self.play(AnimationGroup(
            width_fade.animate(run_time=2, rate_func=smooth).set_value(0),
            width_opacity.animate(run_time=2, rate_func=smooth).set_value(0),
            goal_fade.animate(run_time=2, rate_func=smooth).set_value(0),
            outgoing.animate(run_time=2, rate_func=smooth).set_value(0),
            FadeIn(region, run_time=2, rate_func=smooth),
            travel.animate(run_time=2, rate_func=smooth).set_value(1),
            readout.animate_phase(REFINEMENTS + 1),
            rate_func=linear,
        ))
        self.remove(*active)
        self.wait(2)
