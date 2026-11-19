"""Noon lesson inspired by Grant Sanderson's 3Blue1Brown Fourier scenes.

https://github.com/3b1b/videos/blob/master/_2019/diffyq/part4/fourier_series_scenes.py
Retained pose and trace preparation follows the Noon experiment in
https://github.com/yongkyuns/noon/issues/2358
The four-term outline below was made for this lesson.
"""
from math import cos, sin, tau

from noon import *


# (signed turns per cycle, initial vector); change a term to change the outline.
TERMS = ((1, 1.37), (-2, 0.58), (4, -0.24j), (-3, 0.13))
DRAW_ORIGIN = -2.75 + 0.2j
TARGET_ORIGIN = 3.0 + 0.2j


def xy(point):
    return (point.real, point.imag, 0)


def chain(phase, origin, terms):
    joints = [origin]
    for turns, coefficient in terms:
        angle = turns * tau * phase
        joints.append(joints[-1] + coefficient * complex(cos(angle), sin(angle)))
    return joints


def trace(points, color, width=3, opacity=1):
    return VMobject(stroke_color=color, stroke_width=width).set_points_as_corners(
        [xy(point) for point in points]).set_object_opacity(opacity)


def label(text, x, y, size=24, color=WHITE):
    return Text(text, font_size=size, color=color).move_to((x, y, 0))


def motion(mobject, positions, turns=0):
    # Dyadic time slices give the pose and path reveal the same retained clock.
    intervals = len(positions) - 1
    moves = []
    for i, position in enumerate(positions[1:], start=1):
        target = mobject.copy()
        if turns:
            target.rotate(turns * tau * i / intervals)
        target.move_to(xy(position))
        moves.append(Transform(mobject, target, run_time=1 / intervals, rate_func=linear))
    return Succession(*moves, rate_func=linear)


def prepare(count):
    intervals = 32 if count < 3 else 64
    frames = [chain(i / intervals, DRAW_ORIGIN, TERMS[:count]) for i in range(intervals + 1)]
    parts, moves = [], []
    for i, (turns, coefficient) in enumerate(TERMS[:count]):
        centers = [frame[i] for frame in frames]
        circle = Circle(radius=abs(coefficient), color=BLUE_B, stroke_width=1.4).move_to(xy(centers[0]))
        line = Line(xy(-coefficient / 2), xy(coefficient / 2), color=GOLD, stroke_width=2.5)
        midpoints = [(row[i] + row[i + 1]) / 2 for row in frames]
        line.move_to(xy(midpoints[0]))
        parts.extend((circle, line))
        if i:
            moves.append(motion(circle, centers))
        moves.append(motion(line, midpoints, turns))
    tips = [row[-1] for row in frames]
    tip = Dot(xy(tips[0]), radius=0.072, color=YELLOW)
    drawn = trace(tips, TEAL_A)
    parts.append(tip)
    moves.extend((motion(tip, tips), Create(drawn, rate_func=linear)))
    return VGroup(*parts), drawn, moves


class FourierEpicycles(Scene):
    def construct(self):
        self.next_section("A circle draws a circle")
        title = label("Can spinning arrows draw this?", 0, 3.15, 42)
        target_points = [chain(i / 128, TARGET_ORIGIN, TERMS)[-1] for i in range(129)]
        target = trace(target_points, GREY_B, 2.3, 0.75)
        target_label = label("Target outline", 3.0, -2.17, 24, GREY_B)
        draw_label = label("The tip draws", -2.75, -2.17, 24, TEAL_A)
        self.play(Write(title), Create(target), FadeIn(target_label), run_time=1.2)
        self.wait(0.75)
        previous = None
        for count, section, description in (
            (1, None, "One arrow makes one circle."),
            (2, "Add a second rotation", "Put a faster, backward rotation on the first tip."),
            (4, "Recover the finer bends", "Four rotations recover the fine bends."),
        ):
            if section:
                self.next_section(section)
            caption = label(description, 0, -3.0, 25, GREY_B)
            rig, drawing, moves = prepare(count)
            if previous is None:
                self.play(FadeIn(draw_label), FadeIn(caption), FadeIn(rig), run_time=0.7)
            else:
                self.play(*(FadeOut(obj) for obj in previous), run_time=0.35)
                self.play(FadeIn(caption), FadeIn(rig), run_time=0.65)
            self.play(*moves, run_time=4.0, rate_func=linear)
            self.wait(0.75)
            previous = (rig, drawing, caption)

        self.next_section("Compare with the target")
        # Copy the completed trace to the target to compare them directly.
        overlay = drawing.copy()
        self.add(overlay)
        final_note = label("Each arrow adds a frequency, a size, and a starting angle.",
                           0, -3.0, 25, TEAL_A)
        self.play(FadeOut(rig), FadeOut(caption), Write(final_note),
                  overlay.animate.shift(xy(TARGET_ORIGIN - DRAW_ORIGIN)), run_time=1.4)
        self.wait(2.0)
