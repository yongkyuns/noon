"""Noon lesson based on Academa's ManimGX Pythagoras by rearrangement.

https://manimgx.academa.ai/gallery/pythagoras/
The rearrangement proof is classical. This script uses Noon's retained triangle
transforms and an independently authored comparison layout.
"""
from noon import *


# Outer square has side a + b. Every triangle has legs a and b.
A, B = 1.54, 2.31
SIDE = A + B


def point(x, y):
    return (x - SIDE / 2, y - SIDE / 2 + 0.24, 0)


def polygon(vertices, color, opacity=0.78):
    return Polygon(*(point(x, y) for x, y in vertices), color=color,
                   fill_opacity=opacity, stroke_width=2.2)


def label(text, x, y, size=24, color=WHITE):
    return Text(text, font_size=size, color=color).move_to((x, y, 0))


class PythagorasRearrangement(Scene):
    def construct(self):
        self.next_section("Find the square on the hypotenuse")
        title = label("What can four triangles prove?", 0, 3.20, 42)
        frame = Square(side_length=SIDE, color=GREY_B, stroke_width=2).move_to((0, 0.24, 0))
        first = [(0, 0), (A, 0), (0, B)]
        second = [(SIDE, 0), (SIDE, A), (A, 0)]
        third = [(SIDE, SIDE), (B, SIDE), (SIDE, A)]
        fourth = [(0, SIDE), (0, B), (B, SIDE)]
        colors = (BLUE_D, PURPLE_D, TEAL_D, MAROON_D)
        triangles = [polygon(v, color) for v, color in zip((first, second, third, fourth), colors)]
        a = MathTex("a", color=BLUE_A).move_to(point(A / 2, -0.27))
        b = MathTex("b", color=BLUE_A).move_to(point(-0.27, B / 2))
        c = MathTex("c", color=GOLD).move_to(point(A / 2 + 0.22, B / 2 + 0.12))
        note = label("Start with one right triangle.", 0, -3.12, 26, GREY_B)
        self.play(Write(title), Create(frame), Create(triangles[0]), run_time=1.25)
        self.play(Write(a), Write(b), Write(c), FadeIn(note), run_time=0.8)
        self.wait(0.85)

        center = polygon([(A, 0), (SIDE, A), (B, SIDE), (0, B)], GOLD, 0.20)
        c2 = MathTex("c^2", color=GOLD).move_to((0, 0.24, 0))
        center_note = label("Four hypotenuses enclose a square of area c².", 0, -3.12, 26, GREY_B)
        self.play(Unwrite(a), Unwrite(b), Unwrite(c), FadeOut(note),
                  LaggedStart(*(Create(t) for t in triangles[1:]), lag_ratio=0.25),
                  run_time=1.3)
        self.play(Create(center), Write(c2), FadeIn(center_note), run_time=0.85)
        self.wait(1.5)

        self.next_section("Move the same four triangles")
        move_note = label("Same outer square. Same four triangles.", 0, -3.12, 26, GREY_B)
        self.play(FadeOut(center), Unwrite(c2), FadeOut(center_note), FadeIn(move_note), run_time=0.75)
        # The moving triangles retain their orientation. The first pair fills
        # the lower-left rectangle and the second fills the upper-right one.
        self.play(triangles[2].animate.shift((-B, -A, 0)),
                  triangles[1].animate.shift((0, B, 0)), run_time=1.15)
        self.play(triangles[3].animate.shift((A, 0, 0)), run_time=0.8)
        self.wait(0.65)

        self.next_section("Compare the uncovered areas")
        a_square = polygon([(0, B), (A, B), (A, SIDE), (0, SIDE)], GOLD, 0.20)
        b_square = polygon([(A, 0), (SIDE, 0), (SIDE, B), (A, B)], GOLD, 0.20)
        a2 = MathTex("a^2", color=GOLD).move_to(point(A / 2, B + A / 2))
        b2 = MathTex("b^2", color=GOLD).move_to(point(A + B / 2, B / 2))
        equation = MathTex("a^2+b^2=c^2", color=GOLD, font_size=59).move_to((0, -3.08, 0))
        area_note = label("Both arrangements leave the same uncovered area.", 0, -3.12, 26, GREY_B)
        self.play(Create(a_square), Create(b_square), Write(a2), Write(b2),
                  FadeOut(move_note), FadeIn(area_note), run_time=0.9)
        self.wait(0.8)
        self.play(FadeOut(area_note), Write(equation), run_time=0.95)
        self.wait(2.0)
