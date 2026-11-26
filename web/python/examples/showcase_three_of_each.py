"""Three complete copies of x + 2, with their terms collected by color."""

from math import atan2, hypot, sqrt

from noon import *


# The two term colors link the equation, the tiles, and the results.
PAPER = color_from_hex("#F2E5CF")
SHEET = color_from_hex("#FFFAEF")
INK = color_from_hex("#283A3A")
VARIABLE_COLOR = color_from_hex("#236259")
CONSTANT_COLOR = color_from_hex("#9D452F")
GLOVE_COLOR = color_from_hex("#FFD17A")
COAT = color_from_hex("#548D80")


def tint(color, background, amount):
    return Color(background.red + (color.red - background.red) * amount,
                 background.green + (color.green - background.green) * amount,
                 background.blue + (color.blue - background.blue) * amount)


MUTED = tint(INK, SHEET, 0.82)
RULE = tint(INK, PAPER, 0.10)
CUFF = tint(INK, COAT, 0.40)
SLEEVE = tint(COAT, SHEET, 0.72)
APRON = tint(COAT, SHEET, 0.39)


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
    return MathTypst(source, font_size=size, color=color).move_to((x, y, 0))


def label(message, x, y, size, color=MUTED):
    return Text(message, font="Arimo", font_size=size,
                color=color).move_to((x, y, 0))


def point_below(term):
    x, y, _ = term.get_center()
    return x, y - term.height / 2 - 0.13, 0


def underline(tip, color):
    x, y, _ = tip
    return Line((x - 0.285, y, 0), (x + 0.285, y, 0),
                stroke_color=color, stroke_width=3.0, stroke_cap="round")


def pointing_pose(dx, dy, side):
    """Draw one mirrored arm with every layer's origin at the shoulder."""
    length = hypot(dx, dy)

    def at(u, across):
        across *= side
        return u * dx - across * dy / length, u * dy + across * dx / length

    sleeve = curve([at(u, v) for u, v in [
        (-0.045, 0.10), (0.14, 0.15), (0.35, 0.035), (0.49, 0.025),
        (0.59, 0.02), (0.66, 0.115), (0.755, 0.115),
        (0.77, 0.06), (0.77, -0.06), (0.75, -0.11),
        (0.62, -0.105), (0.60, -0.22), (0.47, -0.225),
        (0.29, -0.235), (0.09, -0.16), (-0.045, -0.10),
    ]], SLEEVE)
    glove = curve([at(u, v) for u, v in [
        (0.73, -0.098), (0.78, -0.106), (0.805, -0.13), (0.84, -0.12),
        (0.87, -0.125), (0.88, -0.075), (0.875, -0.043),
        (0.91, -0.042), (0.999, -0.048), (1, 0),
        (1, 0.042), (0.925, 0.04), (0.866, 0.043),
        (0.855, 0.085), (0.79, 0.115), (0.73, 0.098),
    ]], GLOVE_COLOR, width=1.85)
    cuff = curve([at(u, v) for u, v in [
        (0.693, 0.117), (0.713, 0.135), (0.745, 0.13), (0.769, 0.123),
        (0.775, 0.06), (0.778, -0.07), (0.767, -0.119),
        (0.744, -0.124), (0.714, -0.127), (0.693, -0.114),
    ]], CUFF, width=1.85)
    fold = curve([at(0.33, -0.13), at(0.39, -0.16), at(0.455, -0.17),
                  at(0.51, -0.138)], edge=CUFF, width=1.6)
    return VGroup(sleeve, fold, glove, cuff)


def resting_arm(shoulder, target, side):
    sx, sy, _ = shoulder
    tx, ty, _ = target
    dx, dy = tx - sx, ty - sy
    length = hypot(dx, dy)
    rest_x = side * 0.32 * length
    rest_y = -sqrt(length * length - rest_x * rest_x)
    sweep = atan2(rest_x * dy - rest_y * dx, rest_x * dx + rest_y * dy)
    return pointing_pose(rest_x, rest_y, side).shift(shoulder), sweep


def coat_and_boots():
    boots = [box(x, -1.57, 0.39, 0.29, 0.10, INK) for x in (-0.26, 0.26)]
    coat = curve([
        (-0.46, 0.10), (-0.23, 0.21), (0.27, 0.21), (0.46, 0.10),
        (0.68, -0.02), (0.65, -0.49), (0.65, -1.09),
        (0.65, -1.40), (0.50, -1.45), (0.0, -1.45),
        (-0.49, -1.45), (-0.65, -1.40), (-0.65, -1.09),
        (-0.65, -0.49), (-0.68, -0.02), (-0.46, 0.10),
    ], COAT)
    apron = box(0, -0.78, 0.88, 0.86, 0.16, APRON, CUFF)
    pocket = box(0, -0.92, 0.46, 0.26, 0.065, SLEEVE)
    buttons = [dot(x, -0.42, 0.045, GLOVE_COLOR) for x in (-0.28, 0.28)]
    return VGroup(*boots, coat, apron, pocket, *buttons)


def face(direction):
    turn = direction * 0.073
    helmet = box(0, 0.50, 1.17, 0.82, 0.28, SLEEVE, INK)
    visor = box(turn, 0.50, 0.88, 0.52, 0.18, INK)
    eyes = [dot(x + turn, 0.54, 0.083, SHEET) for x in (-0.174, 0.174)]
    pupils = [dot(x + turn + 0.034 * direction, 0.55 + 0.026 * abs(direction), 0.038, INK)
              for x in (-0.174, 0.174)]
    smile = curve([(turn - 0.066, 0.408), (turn - 0.032, 0.369),
                   (turn + 0.032, 0.369), (turn + 0.066, 0.408)],
                  edge=APRON, width=1.45)
    stem = curve([(-0.20, 0.865), (-0.23, 0.94), (-0.23, 1.006), (-0.18, 1.057)])
    head = VGroup(stem, dot(-0.18, 1.057, 0.060, GLOVE_COLOR), helmet, visor, *eyes, *pupils, smile)
    return head.rotate(-0.10 * direction, about_point=(0, 0.24, 0))


def copies_of_group():
    cards, variable_tiles, constant_tiles = [], [], []
    card_fill = tint(PAPER, SHEET, 0.53)
    y = 1.06
    for x in (0.03, 2.46, 4.89):
        variable = VGroup(
            box(x - 0.55, y, 0.72, 0.60, 0.11, VARIABLE_COLOR),
            formula("x", x - 0.55, y + 0.005, 39, SHEET),
        )
        constant = VGroup(
            box(x + 0.55, y, 0.72, 0.60, 0.11, CONSTANT_COLOR),
            dot(x + 0.43, y, 0.066, SHEET), dot(x + 0.67, y, 0.066, SHEET),
        )
        cards.append(VGroup(
            box(x, y, 2.11, 0.92, 0.17, card_fill, RULE),
            variable, formula("+", x, y, 29), constant,
        ))
        variable_tiles.append(variable.copy())
        constant_tiles.append(constant.copy())
    return cards, variable_tiles, constant_tiles


def collect_tiles(tiles, center):
    return LaggedStart(*[
        Transform(tile, tile.copy().move_to((center + offset, -0.45, 0)))
        for tile, offset in zip(tiles, (-0.85, 0, 0.85))
    ], lag_ratio=0.23)


class ThreeOfEach(Scene):
    def construct(self):
        paper = Rectangle(width=14.4, height=8.3, fill=PAPER, fill_opacity=1, stroke=None)
        sheet = box(0, 0, 13.24, 7.03, 0.31, SHEET, RULE, stroke_width=1.5)
        title = MarkupText("<b>Three of each</b>", font="Libertinus Serif", font_size=48,
                           color=INK).move_to((-3.92, 2.87, 0))
        subtitle = label("Distribute the 3 to both terms", -3.70, 2.31, 25)
        x_term = formula("x", -3.96, 1.49, 81, VARIABLE_COLOR)
        two_term = formula("2", -2.28, 1.60, 81, CONSTANT_COLOR)
        original = VGroup(
            formula("3", -5.26, 1.60, 81),
            formula("(", -4.61, 1.57, 81), x_term,
            formula("+", -3.14, 1.54, 81), two_term, formula(")", -1.71, 1.57, 81),
        )
        x_tip, two_tip = point_below(x_term), point_below(two_term)
        x_mark = underline(x_tip, VARIABLE_COLOR)
        two_mark = underline(two_tip, CONSTANT_COLOR)
        center = (x_tip[0] + two_tip[0]) / 2
        left_shoulder, right_shoulder = (center - 0.49, 0.02, 0), (center + 0.49, 0.02, 0)
        left_arm, left_sweep = resting_arm(left_shoulder, x_tip, -1)
        right_arm, right_sweep = resting_arm(right_shoulder, two_tip, 1)
        left_reach, right_reach = self.value_tracker(0), self.value_tracker(0)
        robot = coat_and_boots().shift((center, 0, 0))
        head = face(0).shift((center, 0, 0))
        shadow = Ellipse(width=1.82, height=0.21, fill=tint(INK, PAPER, 0.045), fill_opacity=1,
                         stroke=None).move_to((center, -1.72, 0))
        divider = Line((-1.37, 2.20, 0), (-1.37, -1.67, 0), stroke_color=RULE, stroke_width=1.3)
        copy_label = formula('3 "copies of" (x + 2)', 2.46, 1.95, 42)
        collect_label = label("Collect like terms", 2.46, -1.82, 25)
        cards, x_tiles, two_tiles = copies_of_group()
        x_panel = box(0.85, -0.69, 2.75, 1.63, 0.20, tint(VARIABLE_COLOR, SHEET, 0.15))
        two_panel = box(4.07, -0.69, 2.75, 1.63, 0.20, tint(CONSTANT_COLOR, SHEET, 0.15))
        product_x = formula("3 x", 0.85, -1.04, 48, VARIABLE_COLOR)
        plus = formula("+", 2.46, -0.68, 42)
        product_two = formula("6", 4.07, -1.04, 48, CONSTANT_COLOR)
        answer = VGroup(
            box(0, -2.66, 7.80, 0.91, 0.19, INK),
            formula("3 (x + 2) = 3 x + 6", 0, -2.65, 56, SHEET),
        )
        self.add(paper, sheet, title, subtitle, original, divider, copy_label,
                 shadow, left_arm, right_arm, robot, head)
        # The layers share a rotation signal and a shoulder origin.
        for part in left_arm:
            self.bind_sampled(part, left_reach, "rotation", [(0, 0), (1, left_sweep)], linear=True)
        for part in right_arm:
            self.bind_sampled(part, right_reach, "rotation", [(0, 0), (1, right_sweep)], linear=True)
        self.wait(0.45)
        self.play(LaggedStart(FadeIn(cards[0]), FadeIn(cards[1]), FadeIn(cards[2]),
                              lag_ratio=0.30), run_time=0.95)
        self.play(left_reach.animate.set_value(1), Transform(head, face(-1).shift((center, 0, 0))),
                  Create(x_mark), run_time=0.72)
        self.add(x_panel, *x_tiles)
        self.play(collect_tiles(x_tiles, 0.85), FadeIn(collect_label), run_time=0.85)
        self.play(FadeIn(product_x), run_time=0.27)
        self.wait(0.64)
        self.play(left_reach.animate.set_value(0), right_reach.animate.set_value(1),
                  Transform(head, face(1).shift((center, 0, 0))),
                  FadeOut(x_mark), Create(two_mark), run_time=0.72)
        self.add(two_panel, *two_tiles)
        self.play(collect_tiles(two_tiles, 4.07), FadeIn(plus), run_time=0.85)
        self.play(FadeIn(product_two), run_time=0.27)
        self.wait(0.62)
        self.play(right_reach.animate.set_value(0), FadeOut(two_mark),
                  Transform(head, face(0).shift((center, 0, 0))), FadeIn(answer), run_time=0.72)
        self.wait(1.45)
