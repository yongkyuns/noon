# Copyright (c) 2026 Academa, Inc.
# SPDX-License-Identifier: MIT
# See THIRD_PARTY_NOTICES.md for license and provenance.
# Source: https://manimgx.academa.ai/gallery/pythagoras/
# https://github.com/academa-labs/manimgx/blob/1fb4dd7327bf5ed7d0c40d022c05cb816455ae1d/examples/pythagoras.py
# The original colored sides, arrangement, math labels, and timings are kept.
# The opaque triangles mask the red square during the slides. Noon interpolates
# a short sequence of rigid poses for each turn about the square's center.
"""a² + b² = c², by moving four triangles.

Put four copies of a right triangle, with legs a and b and hypotenuse c, inside a square of
side a + b, each in a corner and turned a quarter turn from the last: the space they leave is a
tilted square on the hypotenuse, of area c². Now slide three of them, without turning, until
the four pair up into two rectangles: the space left is a square of side a and a square of side
b. The big square and the four triangles never changed, so neither did the space they leave:
a² + b² = c².
"""

import numpy as np

from noon import *

A, B = 2.4, 3.6  # the legs, in scene units
S = A + B  # the big square's side
CORNER = np.array([-5.75, -3.0, 0.0])  # the big square's lower left corner
CENTER = CORNER + np.array([S / 2, S / 2, 0.0])
A_COLOR, B_COLOR, C_COLOR = TEAL, GOLD, RED
TRIANGLE_FILL = BLUE_E
EDGE = 6  # the triangles' edges, in hundredths of a unit
OPACITY = 0.65  # the fill of the space left


def unit(vector):
    return vector / np.linalg.norm(vector)


def at(x: float, y: float) -> np.ndarray:
    """The point (x, y) of the big square, measured from its lower left corner."""
    return CORNER + np.array([x, y, 0.0])


def triangle(right: np.ndarray, a_end: np.ndarray, b_end: np.ndarray) -> VGroup:
    """A right triangle with its right angle at `right` and its legs a and b ending at
    `a_end` and `b_end`: its body, its edges a, b and c, each in its color, and the mark
    of its right angle."""
    body = Polygon(right, a_end, b_end, stroke_width=0)
    body.set_fill(TRIANGLE_FILL, opacity=1.0)
    along_a, along_b = unit(a_end - right), unit(b_end - right)
    mark = VMobject(stroke_color=GREY_A, stroke_width=3)
    size = 0.3
    mark.set_points_as_corners(
        [
            right + size * along_a,
            right + size * (along_a + along_b),
            right + size * along_b,
        ]
    )
    return VGroup(
        body,
        Line(right, a_end, color=A_COLOR, stroke_width=EDGE),
        Line(right, b_end, color=B_COLOR, stroke_width=EDGE),
        Line(a_end, b_end, color=C_COLOR, stroke_width=EDGE),
        mark,
    )


def quarter_turn(triangle):
    # All triangle parts follow the same arc. Take small rigid pose steps
    # around the outer square's center so the corners track one another.
    steps = 16
    steps_per_part = []
    for part in triangle:
        steps_per_part.append(Succession(*(
            Transform(part, part.copy().rotate(PI * k / (2 * steps), about_point=CENTER),
                      run_time=1 / steps, rate_func=linear)
            for k in range(1, steps + 1)
        ), rate_func=linear))
    return steps_per_part


def label(
    tex: str, color: ManimColor, x: float, y: float, size: int = 60
) -> MathTex:
    return MathTex(tex, font_size=size, color=color).move_to(at(x, y))


class Pythagoras(Scene):
    def copy_for_equation(self, source, part):
        flying = source.copy()
        self.add(flying)
        return flying, flying.animate.scale(part.height / flying.height).move_to(part)

    def construct(self) -> None:
        outline = Polygon(at(0, 0), at(S, 0), at(S, S), at(0, S))
        outline.set_stroke(GREY_B, width=4)

        # one triangle in the lower right corner, then three copies, each turned a quarter
        # turn about the square's center from the last
        first = triangle(at(S, 0), at(S, A), at(A, 0))
        a_label = label("a", A_COLOR, S + 0.4, A / 2)
        b_label = label("b", B_COLOR, A + B / 2, -0.45)
        hypotenuse_mid = (at(S, A) + at(A, 0)) / 2
        c_label = MathTex("c", font_size=60, color=C_COLOR)
        c_label.move_to(hypotenuse_mid + 0.42 * unit(CENTER - hypotenuse_mid))

        self.next_section("Color the sides of the triangle")
        self.play(Create(outline), run_time=1.0)
        self.play(
            FadeIn(first[0]),
            *(Create(part) for part in first[1:]),
            run_time=1.2,
        )
        self.play(Write(a_label), Write(b_label), Write(c_label), run_time=0.8)
        triangles = [first]
        for _ in range(3):
            turned = triangles[-1].copy()
            self.add(turned)
            self.play(*quarter_turn(turned), run_time=0.8)
            triangles.append(turned)
        _, upper_right, upper_left, lower_left = triangles

        # the other halves of the two labeled sides: each side is a + b
        self.play(
            FadeIn(label("a", A_COLOR, A / 2, -0.45)),
            FadeIn(label("b", B_COLOR, S + 0.4, A + B / 2)),
            run_time=0.6,
        )

        self.next_section("The square on the hypotenuse")
        tilted = Polygon(at(A, 0), at(S, A), at(B, S), at(0, B), stroke_width=0)
        tilted.set_fill(C_COLOR, opacity=OPACITY)
        c_squared = MathTex("c^2", font_size=96, color=C_COLOR).move_to(CENTER)
        self.play(
            FadeIn(tilted), FadeOut(c_label), Write(c_squared), run_time=1.2
        )

        equation = MathTex("{{a^2}} + {{b^2}} = {{c^2}}", font_size=96)
        equation.move_to(np.array([4.0, 0.0, 0.0]))
        eq_a, plus, eq_b, equals, eq_c = equation
        eq_a.set_color(A_COLOR)
        eq_b.set_color(B_COLOR)
        eq_c.set_color(C_COLOR)
        copy_c, rise_c = self.copy_for_equation(c_squared[0], eq_c)
        self.play(rise_c, run_time=1.5)
        self.remove(copy_c)
        self.add(eq_c)
        self.wait(0.5)

        # three slides, none turning: the space left flows around each triangle as it moves
        self.next_section("Rearrange the pieces")
        left = Polygon(at(0, 0), at(S, 0), at(S, S), at(0, S), stroke_width=0)
        left.set_fill(C_COLOR, opacity=OPACITY)
        self.remove(tilted)
        self.add(left)
        self.bring_to_back(left)
        slides = [
            (upper_left, at(A, -B) - at(0, 0)),  # across the tilted square, by c
            (upper_right, at(-B, 0) - at(0, 0)),  # left along the top, by b
            (lower_left, at(0, A) - at(0, 0)),  # up along the side, by a
        ]
        self.play(
            upper_left.animate(run_time=1.6).shift(slides[0][1]),
            FadeOut(c_squared, run_time=0.5),
        )
        for moving, by in slides[1:]:
            self.play(moving.animate.shift(by), run_time=1.6)

        self.next_section("The squares on the legs")
        a_square = Polygon(at(0, 0), at(A, 0), at(A, A), at(0, A), stroke_width=0)
        b_square = Polygon(at(A, A), at(S, A), at(S, S), at(A, S), stroke_width=0)
        for square in (a_square, b_square):
            square.set_fill(C_COLOR, opacity=OPACITY)
        self.remove(left)
        self.add(a_square, b_square, *triangles)
        a_squared = MathTex("a^2", font_size=80, color=A_COLOR).move_to(a_square)
        b_squared = MathTex("b^2", font_size=96, color=B_COLOR).move_to(b_square)
        self.play(
            a_square.animate.set_fill(A_COLOR, opacity=OPACITY),
            b_square.animate.set_fill(B_COLOR, opacity=OPACITY),
            Write(a_squared),
            Write(b_squared),
            run_time=1.2,
        )

        # the equation, from the areas' labels
        copy_a, rise_a = self.copy_for_equation(a_squared[0], eq_a)
        self.play(rise_a, FadeIn(plus), run_time=1.3)
        self.remove(copy_a)
        self.add(eq_a)
        copy_b, rise_b = self.copy_for_equation(b_squared[0], eq_b)
        self.play(rise_b, FadeIn(equals), run_time=1.3)
        self.remove(copy_b)
        self.add(eq_b)
        self.wait(2.0)
