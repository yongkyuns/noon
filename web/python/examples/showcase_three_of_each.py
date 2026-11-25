"""A printmaker points to each term while three copies of it appear."""

from math import atan2, cos, hypot, sin, sqrt

from noon import *


PAPER = "#F2E5CF"
SHEET = "#FFFAEF"
INK = "#283A3A"
MUTED = "#506461"
TEAL = "#236259"
RUST = "#9D452F"
GOLD = "#FFD17A"
COAT = "#548D80"
CUFF = "#3C6E65"


def box(x, y, width, height, radius, fill, edge=None, stroke_width=1.8):
    return RoundedRectangle(
        width=width, height=height, corner_radius=radius, fill=fill, fill_opacity=1,
        stroke=edge, stroke_width=stroke_width if edge else 0,
    ).move_to((x, y, 0))


def dot(x, y, radius, fill):
    return Circle(radius=radius, fill=fill, fill_opacity=1, stroke=None).move_to((x, y, 0))


def curve(points, fill=None, edge=INK, width=2.0):
    points = [(x, y, 0) for x, y in points]
    art = VMobject(
        fill_color=fill, fill_opacity=1 if fill else 0,
        stroke_color=edge, stroke_width=width, stroke_cap="round", stroke_join="round",
    ).start_new_path(points[0])
    for i in range(1, len(points), 3):
        art.add_cubic_bezier_curve_to(*points[i:i + 3])
    return art.close_path() if fill else art


def formula(source, x, y, size, color=INK):
    return MathTypst(source, font_size=size, color=color_from_hex(color)).move_to((x, y, 0))


def label(message, x, y, size, color=MUTED):
    return Text(message, font="Arimo", font_size=size,
                color=color_from_hex(color)).move_to((x, y, 0))


def point_below(term):
    x, y, _ = term.get_center()
    return x, y - term.height / 2 - 0.13, 0


def underline(tip, color):
    x, y, _ = tip
    return Line((x - 0.285, y, 0), (x + 0.285, y, 0),
                stroke_color=color, stroke_width=3.0, stroke_cap="round")


def pointing_pose(shoulder, tip, side):
    """Place the same sleeve and glove on either side of the coat."""
    sx, sy, _ = shoulder
    tx, ty, _ = tip
    dx, dy = tx - sx, ty - sy
    length = hypot(dx, dy)

    def at(u, across):
        across *= side
        return sx + u * dx - across * dy / length, sy + u * dy + across * dx / length

    sleeve = curve([at(u, v) for u, v in [
        (-0.045, 0.10), (0.14, 0.15), (0.35, 0.035), (0.49, 0.025),
        (0.59, 0.02), (0.66, 0.115), (0.755, 0.115),
        (0.77, 0.06), (0.77, -0.06), (0.75, -0.11),
        (0.62, -0.105), (0.60, -0.22), (0.47, -0.225),
        (0.29, -0.235), (0.09, -0.16), (-0.045, -0.10),
    ]], "#85B2A0")
    glove = curve([at(u, v) for u, v in [
        (0.73, -0.098), (0.78, -0.106), (0.805, -0.13), (0.84, -0.12),
        (0.87, -0.125), (0.88, -0.075), (0.875, -0.043),
        (0.91, -0.042), (0.999, -0.048), (1, 0),
        (1, 0.042), (0.925, 0.04), (0.866, 0.043),
        (0.855, 0.085), (0.79, 0.115), (0.73, 0.098),
    ]], GOLD, width=1.85)
    cuff = curve([at(u, v) for u, v in [
        (0.693, 0.117), (0.713, 0.135), (0.745, 0.13), (0.769, 0.123),
        (0.775, 0.06), (0.778, -0.07), (0.767, -0.119),
        (0.744, -0.124), (0.714, -0.127), (0.693, -0.114),
    ]], CUFF, width=1.85)
    fold = curve([at(0.33, -0.13), at(0.39, -0.16), at(0.455, -0.17),
                  at(0.51, -0.138)], edge=CUFF, width=1.6)
    return VGroup(sleeve, fold, glove, cuff)


def resting_tip(shoulder, target, side):
    sx, sy, _ = shoulder
    tx, ty, _ = target
    length = hypot(tx - sx, ty - sy)
    rest_x = side * 0.32 * length
    return sx + rest_x, sy - sqrt(length * length - rest_x * rest_x), 0


def swing_targets(shoulder, start, end, side):
    sx, sy, _ = shoulder
    ax, ay = start[0] - sx, start[1] - sy
    bx, by = end[0] - sx, end[1] - sy
    sweep = atan2(ax * by - ay * bx, ax * bx + ay * by)
    # Sample the swing around the shoulder so the intermediate drawings
    # keep the hand clear of the coat.
    poses = []
    for fraction in (1 / 3, 2 / 3):
        c, s = cos(sweep * fraction), sin(sweep * fraction)
        poses.append((sx + c * ax - s * ay, sy + s * ax + c * ay, 0))
    poses.append(end)
    return [pointing_pose(shoulder, tip, side) for tip in poses]


def coat_and_boots():
    boots = [box(x, -1.57, 0.39, 0.29, 0.10, INK) for x in (-0.26, 0.26)]
    coat = curve([
        (-0.46, 0.10), (-0.23, 0.21), (0.27, 0.21), (0.46, 0.10),
        (0.68, -0.02), (0.65, -0.49), (0.65, -1.09),
        (0.65, -1.40), (0.50, -1.45), (0.0, -1.45),
        (-0.49, -1.45), (-0.65, -1.40), (-0.65, -1.09),
        (-0.65, -0.49), (-0.68, -0.02), (-0.46, 0.10),
    ], COAT)
    apron = box(0, -0.78, 0.88, 0.86, 0.16, "#BBD2C1", CUFF)
    pocket = box(0, -0.92, 0.46, 0.26, 0.065, "#8FB7A4")
    buttons = [dot(x, -0.42, 0.045, GOLD) for x in (-0.28, 0.28)]
    return VGroup(*boots, coat, apron, pocket, *buttons)


def face(direction):
    turn = direction * 0.073
    helmet = box(0, 0.50, 1.17, 0.82, 0.28, "#7BAD9F", INK)
    visor = box(turn, 0.50, 0.88, 0.52, 0.18, INK)
    eyes = [dot(x + turn, 0.54, 0.083, "#FFF3D7") for x in (-0.174, 0.174)]
    pupils = [dot(x + turn + 0.034 * direction, 0.55 + 0.026 * abs(direction), 0.038, INK)
              for x in (-0.174, 0.174)]
    smile = curve([(turn - 0.066, 0.408), (turn - 0.032, 0.369),
                   (turn + 0.032, 0.369), (turn + 0.066, 0.408)],
                  edge="#A7C9BA", width=1.45)
    stem = curve([(-0.20, 0.865), (-0.23, 0.94), (-0.23, 1.006), (-0.18, 1.057)])
    head = VGroup(stem, dot(-0.18, 1.057, 0.060, GOLD), helmet, visor, *eyes, *pupils, smile)
    return head.rotate(-0.10 * direction, about_point=(0, 0.24, 0))


def copy_panel(x, accent, tint, variable):
    caption = "One x per copy" if variable else "Two dots per copy"
    expression = "3 x" if variable else "3 dot 2 = 6"
    panel = box(x, -0.53, 3.62, 2.09, 0.22, tint)
    heading = label(caption, x, 0.11, 28, accent)
    slots = VGroup(*[box(x + offset, -0.49, 0.87, 0.69, 0.12, SHEET)
                     for offset in (-1.02, 0, 1.02)])
    marks = []
    for offset in (-1.02, 0, 1.02):
        center = x + offset
        stamp = box(center, -0.49, 0.78, 0.61, 0.11, accent)
        if variable:
            mark = formula("x", center, -0.48, 39, SHEET)
        else:
            mark = VGroup(dot(center - 0.146, -0.49, 0.076, SHEET),
                          dot(center + 0.146, -0.49, 0.076, SHEET))
        marks.append(VGroup(stamp, mark))
    return VGroup(panel, heading, slots), marks, formula(expression, x, -1.13, 45, accent)


class ThreeOfEach(Scene):
    def construct(self):
        paper = Rectangle(width=14.4, height=8.3, fill=PAPER, fill_opacity=1, stroke=None)
        sheet = box(0, 0, 13.24, 7.03, 0.31, SHEET, "#DECDB0", stroke_width=1.5)
        title = MarkupText("<b>Three of each</b>", font="Libertinus Serif", font_size=48,
                           color=color_from_hex(INK)).move_to((-3.92, 2.87, 0))
        subtitle = label("Distribute the 3 to both terms", -3.70, 2.31, 25)
        x_term = formula("x", -0.68, 1.49, 81, TEAL)
        two_term = formula("2", 1.00, 1.60, 81, RUST)
        original = VGroup(
            formula("3", -1.98, 1.60, 81, "#8A5E29"),
            formula("(", -1.33, 1.57, 81), x_term,
            formula("+", 0.14, 1.54, 81), two_term, formula(")", 1.57, 1.57, 81),
        )
        x_tip, two_tip = point_below(x_term), point_below(two_term)
        x_mark = underline(x_tip, TEAL)
        two_mark = underline(two_tip, RUST)
        center = (x_tip[0] + two_tip[0]) / 2
        left_shoulder, right_shoulder = (center - 0.49, 0.02, 0), (center + 0.49, 0.02, 0)
        left_rest = resting_tip(left_shoulder, x_tip, -1)
        right_rest = resting_tip(right_shoulder, two_tip, 1)
        left_arm = pointing_pose(left_shoulder, left_rest, -1)
        right_arm = pointing_pose(right_shoulder, right_rest, 1)
        reach_left = swing_targets(left_shoulder, left_rest, x_tip, -1)
        reach_right = swing_targets(right_shoulder, right_rest, two_tip, 1)
        lower_left = (reach_left[1], reach_left[0], pointing_pose(left_shoulder, left_rest, -1))
        lower_right = (reach_right[1], reach_right[0], pointing_pose(right_shoulder, right_rest, 1))
        robot = coat_and_boots().shift((center, 0, 0))
        head = face(0).shift((center, 0, 0))
        shadow = Ellipse(width=1.82, height=0.21, fill="#EBDFC9", fill_opacity=1,
                         stroke=None).move_to((center, -1.72, 0))
        x_panel, x_tiles, product_x = copy_panel(-4.42, TEAL, "#DFEEE5", True)
        two_panel, two_tiles, product_two = copy_panel(4.42, RUST, "#F8E5DB", False)
        answer = VGroup(
            box(0, -2.86, 7.80, 0.91, 0.19, INK),
            formula("3 (x + 2) = 3 x + 6", 0, -2.85, 56, SHEET),
        )
        self.add(paper, sheet, title, subtitle, original, x_panel, two_panel,
                 shadow, left_arm, right_arm, robot, head)
        self.wait(0.45)
        self.play(Transform(left_arm, reach_left[0]),
                  Transform(head, face(-1).shift((center, 0, 0))), run_time=0.30, rate_func=linear)
        self.play(Transform(left_arm, reach_left[1]), run_time=0.25, rate_func=linear)
        self.play(Transform(left_arm, reach_left[2]), Create(x_mark), run_time=0.30, rate_func=linear)
        self.play(LaggedStart(FadeIn(x_tiles[0]), FadeIn(x_tiles[1]), FadeIn(x_tiles[2]),
                              FadeIn(product_x), lag_ratio=0.38), run_time=0.95)
        self.wait(0.60)
        self.play(Transform(left_arm, lower_left[0]), Transform(right_arm, reach_right[0]),
                  Transform(head, face(1).shift((center, 0, 0))),
                  FadeOut(x_mark), run_time=0.27, rate_func=linear)
        self.play(Transform(left_arm, lower_left[1]), Transform(right_arm, reach_right[1]),
                  run_time=0.24, rate_func=linear)
        self.play(Transform(left_arm, lower_left[2]), Transform(right_arm, reach_right[2]),
                  Create(two_mark), run_time=0.27, rate_func=linear)
        self.play(LaggedStart(FadeIn(two_tiles[0]), FadeIn(two_tiles[1]), FadeIn(two_tiles[2]),
                              FadeIn(product_two), lag_ratio=0.38), run_time=0.95)
        self.wait(0.64)
        self.play(Transform(right_arm, lower_right[0]), FadeOut(two_mark),
                  Transform(head, face(0).shift((center, 0, 0))), run_time=0.32, rate_func=linear)
        self.play(Transform(right_arm, lower_right[1]), run_time=0.27, rate_func=linear)
        self.play(Transform(right_arm, lower_right[2]), FadeIn(answer), run_time=0.31, rate_func=linear)
        self.wait(1.40)
