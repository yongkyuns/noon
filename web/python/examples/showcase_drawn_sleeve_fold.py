"""Morph three illustrated arm poses around a coat in a 2D scene."""

from math import cos, sin

from noon import *


PAPER = "#F7ECD7"
INK = "#303047"
COAT = "#4B8279"
LINING = "#36645F"
SLEEVE = "#78AA93"
HIGHLIGHT = "#A0C1A2"
GLOVE = "#FFD075"
CREASE = "#B35A40"
CREAM = "#FFF0CC"


# Matching cubic segments trace each silhouette from shoulder to cuff and
# back. The two end caps use the same wrist coordinates as the cuff and glove.
REACH = {
    "wrist": (2.23, 0.62), "heading": 0.44, "cuff_width": 1, "glove_length": 1,
    "outer": ((0.91, 0.66), (1.06, 0.27), (1.42, 0.34), (1.66, 0.39), (1.89, 0.62)),
    "inner": ((1.94, 0.25), (1.54, -0.06), (1.24, -0.01), (1.00, 0.02), (0.77, 0.24)),
}
TURN = {
    "wrist": (1.45, -0.53), "heading": -2.13, "cuff_width": 0.68, "glove_length": 0.69,
    "outer": ((0.93, 0.69), (1.70, 0.43), (1.77, 0.20), (1.84, 0.01), (1.72, -0.35)),
    "inner": ((1.34, -0.32), (1.47, -0.16), (1.29, -0.06), (1.04, 0.06), (0.77, 0.24)),
}
TUCK = {
    "wrist": (0.34, -0.44), "heading": -3.33, "cuff_width": 0.89, "glove_length": 0.92,
    "outer": ((0.90, 0.67), (1.64, 0.23), (1.48, -0.28), (1.37, -0.66), (0.88, -0.70)),
    "inner": ((0.76, -0.30), (1.20, -0.38), (1.10, -0.18), (1.00, -0.01), (0.77, 0.23)),
}


def path(points, fill, *, stroke=INK, width=2.6):
    points = [(x, y, 0) for x, y in points]
    outline = VMobject(
        fill_color=fill, fill_opacity=1 if fill else 0,
        stroke_color=stroke, stroke_width=width if stroke else 0,
        stroke_cap="round", stroke_join="round",
    ).start_new_path(points[0])
    for i in range(1, len(points), 3):
        outline.add_cubic_bezier_curve_to(*points[i:i + 3])
    return outline.close_path() if fill else outline


def dot(x, y, radius, color):
    return Circle(radius=radius, fill_color=color, fill_opacity=1, stroke=None).move_to((x, y, 0))


def round_box(x, y, width, height, radius, fill, stroke=INK):
    return RoundedRectangle(
        width=width, height=height, corner_radius=radius,
        fill_color=fill, fill_opacity=1, stroke=stroke, stroke_width=2.6 if stroke else 0,
    ).move_to((x, y, 0))


def sleeve_fold(pose):
    c, s = cos(pose["heading"]), sin(pose["heading"])
    x, y = pose["wrist"]

    def at(u, v):
        v *= pose["cuff_width"]
        return (x + c * u - s * v, y + s * u + c * v)

    outer_tip, inner_tip = at(-0.09, 0.17), at(-0.09, -0.17)
    sleeve = path([
        (0.54, 0.63), *pose["outer"], outer_tip,
        at(-0.07, 0.12), at(-0.07, -0.12), inner_tip,
        *pose["inner"], (0.54, 0.29),
    ], SLEEVE)
    cuff = path([at(u, v) for u, v in [
        (-0.20, 0.17), (-0.12, 0.16), (-0.01, 0.15), (0.075, 0.145),
        (0.11, 0.10), (0.11, -0.10), (0.075, -0.145),
        (-0.02, -0.15), (-0.13, -0.16), (-0.20, -0.17),
        (-0.17, -0.08), (-0.17, 0.08), (-0.20, 0.17),
    ]], LINING, width=2.1)
    glove = path([at(u * pose["glove_length"], v) for u, v in [
        (-0.03, -0.103), (0.06, -0.115), (0.16, -0.105), (0.24, -0.16),
        (0.39, -0.19), (0.53, -0.15), (0.55, -0.034),
        (0.57, 0.08), (0.48, 0.17), (0.34, 0.158),
        (0.27, 0.156), (0.22, 0.119), (0.18, 0.143),
        (0.13, 0.23), (0.05, 0.20), (0.035, 0.118),
        (-0.01, 0.105), (-0.035, 0.09), (-0.03, -0.103),
    ]], GLOVE)
    thumb = path([at(u * pose["glove_length"], v) for u, v in [
        (0.18, 0.143), (0.17, 0.112), (0.19, 0.062), (0.26, 0.047),
    ]], None, stroke=CREASE, width=1.65)
    stitch = path([at(u, v) for u, v in [
        (-0.06, -0.105), (-0.04, -0.066), (-0.04, 0.066), (-0.06, 0.105),
    ]], None, stroke=HIGHLIGHT, width=1.5)
    return VGroup(sleeve, glove, thumb, cuff, stitch)


def coat_and_head():
    coat = path([
        (-0.70, 0.73), (-0.48, 0.91), (0.49, 0.90), (0.71, 0.74),
        (0.96, 0.54), (0.93, 0.16), (0.96, -0.38),
        (1.00, -1.06), (0.84, -1.22), (0.47, -1.23),
        (0.21, -1.27), (-0.27, -1.27), (-0.52, -1.23),
        (-0.86, -1.20), (-0.98, -1.03), (-0.95, -0.37),
        (-0.93, 0.16), (-0.95, 0.55), (-0.70, 0.73),
    ], COAT)
    lapels = path([
        (-0.70, 0.69), (-0.48, 0.82), (0.46, 0.83), (0.70, 0.69),
        (0.53, 0.48), (0.26, 0.43), (0, 0.26),
        (-0.26, 0.43), (-0.53, 0.48), (-0.70, 0.69),
    ], LINING, width=1.75)
    seam = Line((0, 0.26, 0), (0, -1.23, 0), stroke_color=LINING, stroke_width=2.3)
    pockets = [
        path([(x, -0.50), (x * 0.74, -0.58), (x * 0.60, -0.62), (x * 0.56, -0.72)],
             None, stroke=LINING, width=3.8)
        for x in (-0.69, 0.69)
    ]
    buttons = [dot(-0.155, y, 0.042, GLOVE) for y in (0.08, -0.25, -0.58, -0.91)]
    helmet = round_box(0, 1.42, 1.61, 1.12, 0.42, COAT)
    visor = round_box(0.05, 1.43, 1.29, 0.72, 0.27, INK, stroke=None)
    reflection = path([(-0.39, 1.66), (-0.31, 1.70), (-0.17, 1.70), (-0.10, 1.70)],
                      None, stroke="#65687A", width=1.7)
    eyes = [round_box(x, 1.43, 0.133, 0.193, 0.06, CREAM, stroke=None) for x in (-0.18, 0.30)]
    smile = path([(0.02, 1.29), (0.06, 1.245), (0.12, 1.245), (0.16, 1.29)],
                 None, stroke=HIGHLIGHT, width=1.6)
    antenna = path([(-0.47, 1.94), (-0.45, 2.04), (-0.48, 2.15), (-0.55, 2.20)],
                   None, width=3.0)
    return VGroup(coat, lapels, seam, *pockets, *buttons, antenna,
                  dot(-0.55, 2.21, 0.079, GLOVE), helmet, visor, reflection, *eyes, smile)


def place(drawing):
    return drawing.scale(1.62, about_point=(0, 0, 0)).shift((-0.83, -0.27, 0))


class DrawnSleeveFold(Scene):
    def construct(self):
        self.add(Rectangle(width=14.4, height=8.2, fill=PAPER, fill_opacity=1, stroke=None))
        shadow = Ellipse(width=2.38, height=0.30, fill="#E5D3B8", fill_opacity=1,
                         stroke=None).move_to((0, -1.86, 0))
        boots = [round_box(x, -1.51, 0.50, 0.67, 0.18, INK) for x in (-0.50, 0.50)]
        other_arm = path([
            (-0.60, 0.65), (-1.12, 0.58), (-1.29, -0.14), (-1.39, -0.51),
            (-1.44, -0.67), (-1.27, -0.77), (-1.14, -0.66),
            (-0.99, -0.47), (-0.83, 0.24), (-0.54, 0.28),
        ], SLEEVE)
        other_glove = round_box(-1.32, -0.71, 0.37, 0.48, 0.18, GLOVE).rotate(-0.34)
        arm = place(sleeve_fold(REACH))
        # The coat stays above the entire arm drawing. Its edge hides the
        # glove, cuff, and sleeve together as the tuck curves inward.
        self.add(place(VGroup(shadow, *boots, other_glove, other_arm)), arm, place(coat_and_head()))
        self.wait(0.55)
        self.play(Transform(arm, place(sleeve_fold(TURN))), run_time=0.77)
        self.play(Transform(arm, place(sleeve_fold(TUCK))), run_time=0.90)
        self.wait(1.35)
        self.play(Transform(arm, place(sleeve_fold(TURN))), run_time=0.90)
        self.play(Transform(arm, place(sleeve_fold(REACH))), run_time=0.77)
        self.wait(0.76)
