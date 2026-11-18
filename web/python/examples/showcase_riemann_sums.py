"""Noon adaptation of ManimGX's Rectangles become an integral (Academa).
Reference: https://manimgx.academa.ai/gallery/riemann-sums/
"""
from math import cos, sin

from noon import *

A = 0.5
B = 4.5  # Try 3.5 to measure a shorter stretch of the same curve.
REFINEMENTS = 6
NIGHT = color_from_hex("#101e29")
CARD = color_from_hex("#192e39")
EDGE = color_from_hex("#3c5864")
INK = color_from_hex("#f5efe2")
MUTED = color_from_hex("#afc9cc")
CURVE = color_from_hex("#ffda89")
STRIP_COLORS = tuple(color_from_hex(color) for color in (
    "#3387b4", "#3699b1", "#43abaf", "#63bd9e",
))


def f(x):
    return 1 + 0.6 * x + 0.9 * sin(1.6 * x)


def antiderivative(x):
    return x + 0.3 * x * x - (0.9 / 1.6) * cos(1.6 * x)


def rectangle_sum(count):
    dx = (B - A) / count
    return dx * sum(f(A + i * dx) for i in range(count))


def label(text, size, x, y, color=INK):
    return Text(text, font_size=size, color=color).move_to((x, y, 0))


def strips(axes, count, *, split=False, rise=1):
    """Prepare four colored paths, each containing a quarter of the strips.

    In the split pose, two adjacent strips reproduce one parent exactly.
    In the final pose, each half takes its own left-edge height.
    """
    dx = (B - A) / count
    colors = [VMobject(fill_color=color, fill_opacity=1, stroke_width=0)
              for color in STRIP_COLORS]
    for index in range(count):
        left = A + index * dx
        sample = left - dx if split and index % 2 else left
        gap = min(0.018, dx / 9)
        left_gap = right_gap = gap
        if split:
            outer = min(0.018, 2 * dx / 9)
            left_gap, right_gap = (0, outer) if index % 2 else (outer, 0)
        start, end = left + left_gap, left + dx - right_gap
        height = rise * f(sample)
        shape = colors[index // (count // len(colors))]
        shape.start_new_path(axes.c2p(start, 0))
        shape.add_line_to(axes.c2p(end, 0))
        shape.add_line_to(axes.c2p(end, height))
        shape.add_line_to(axes.c2p(start, height))
        shape.close_path()
    return colors


def measures(count):
    x = 4.78
    width = (B - A) / count
    return [
        label(f"{count} strips", 30, x, 1.16),
        label(f"Δx = {width:.6g}", 25, x, 0.54, MUTED),
        label(f"{rectangle_sum(count):.3f}", 44, x, -0.89),
    ]


class RiemannSums(Scene):
    def construct(self):
        if not 2 <= B <= 4.75:
            raise ValueError("Choose an upper limit from 2 to 4.75.")
        axes = Axes([0, 5, 1], [0, 5, 1], x_length=8.65, y_length=4.54, tips=False,
                    color=EDGE).move_to((-1.82, -0.1, 0))
        curve = axes.plot(f, [0, 5, 0.025], color=CURVE, stroke_width=4, use_smoothing=False)
        curve.set_z_index(2)
        end_of_curve = axes.c2p(4.97, f(4.97))
        curve_name = label("f(x)", 26, end_of_curve[0] - 0.12, end_of_curve[1] + 0.36, CURVE)
        ends = [Text(name, font_size=26, color=INK).move_to(axes.c2p(x, -0.29))
                for name, x in (("a", A), ("b", B))]
        area = axes.get_area(curve, [A, B], color=STRIP_COLORS[-1], opacity=0.78, stroke_width=0)
        outline = label("How do we measure a curved area?", 42, 0, 3.12)
        prompt = label("Find the area under f(x) between a and b", 29, 0, -3.38, MUTED)
        self.add(Rectangle(width=16, height=9, fill=NIGHT, fill_opacity=1, stroke=None), outline)

        self.next_section("The area under the curve")
        self.play(Create(axes), Create(curve), run_time=1.2)
        self.play(FadeIn(area), Write(curve_name), *(Write(bound) for bound in ends),
                  FadeIn(prompt), run_time=0.8)
        self.wait(1.7)

        self.next_section("Approximate with rectangles")
        panel = RoundedRectangle(width=3.62, height=4.3, corner_radius=0.22, fill=CARD,
                                 fill_opacity=1, stroke=EDGE, stroke_width=1.3)
        panel.move_to((4.78, -0.08, 0))
        stat_title = label("RECTANGLE SUM", 25, 4.78, 1.75, MUTED)
        sum_formula = MathTypst("sum_i f(x_i) Delta x", font_size=31, color=INK).move_to((4.78, -0.18, 0))
        self.play(FadeOut(area), FadeOut(prompt), FadeIn(panel), FadeIn(stat_title), run_time=0.6)
        active = strips(axes, 4, rise=0.02)
        self.add(*active)
        readout = measures(4)
        caption = label("Each strip uses the height at its left edge", 29, 0, -3.38)
        self.play(*(Transform(bar, target) for bar, target in zip(active, strips(axes, 4))),
                  *(FadeIn(item) for item in readout), Write(sum_formula), FadeIn(caption), run_time=1.2)
        self.wait(1.7)

        self.next_section("Split and resample")
        self.play(FadeOut(caption), run_time=0.3)
        dx = (B - A) / 4
        middle = A + dx / 2
        divider = Line(axes.c2p(middle, 0), axes.c2p(middle, f(A)), color=CURVE, stroke_width=2)
        caption = label("The right half gets a new height at the split", 29, 0, -3.38)
        exact = antiderivative(B) - antiderivative(A)
        goal = label(f"Area ≈ {exact:.3f}", 23, 4.78, -1.69, MUTED)
        self.play(Create(divider), FadeIn(caption), FadeIn(goal), run_time=0.7)
        self.wait(0.6)
        for level in range(1, REFINEMENTS + 1):
            count = 4 * 2 ** level
            self.play(*(FadeOut(item) for item in readout), run_time=0.3)
            halves = strips(axes, count, split=True)
            self.remove(*active)
            self.add(*halves)
            active = halves
            motions = [Transform(bar, target) for bar, target in zip(active, strips(axes, count))]
            if level == 1:
                motions.append(FadeOut(divider))
            self.play(*motions, run_time=1.4 if level < 4 else 0.9)
            readout = measures(count)
            self.play(*(FadeIn(item) for item in readout), run_time=0.4)
            self.wait(0.5 if level < 4 else 0.2)
        self.wait(0.7)

        self.next_section("Take the limit")
        self.play(FadeOut(caption), FadeOut(stat_title), FadeOut(sum_formula), FadeOut(goal),
                  *(FadeOut(item) for item in readout), run_time=0.5)
        integral = MathTypst("integral_a^b f(x) dif x", font_size=36, color=INK).move_to((4.78, 0.73, 0))
        value = label(f"≈ {exact:.3f}", 44, 4.78, -0.61)
        final_title = label("INTEGRAL", 25, 4.78, 1.51, MUTED)
        limit = label("As Δx approaches 0, the sum approaches the area", 29, 0, -3.38)
        self.play(FadeIn(area), *(FadeOut(bar) for bar in active), Write(integral),
                  FadeIn(final_title), FadeIn(limit), run_time=1.3)
        self.play(FadeIn(value), run_time=0.5)
        self.wait(3.5)
