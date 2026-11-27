"""Three complete copies of x + 2, with their terms collected by color."""

from noon import *


# The two term colors link the equation, the tiles, and the results.
PAPER = color_from_hex("#F2E5CF")
SHEET = color_from_hex("#FFFAEF")
INK = color_from_hex("#283A3A")
VARIABLE_COLOR = color_from_hex("#236259")
CONSTANT_COLOR = color_from_hex("#9D452F")
GLOVE_COLOR = color_from_hex("#FFD17A")
COAT = color_from_hex("#548D80")

X_TERM = (-3.96, 1.49)
TWO_TERM = (-2.28, 1.60)
FINGERTIP = 0.24


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


def arm(prefix, side):
    upper, forearm = 0.59, 0.59
    shoulder, elbow, wrist = prefix + "_shoulder", prefix + "_elbow", prefix + "_wrist"
    joints = [
        Joint2D(shoulder, "guide", (side * 0.49, 0.02), -PI / 2 + side * 0.58),
        Joint2D(elbow, shoulder, (upper, 0), -side * 0.62),
        Joint2D(wrist, elbow, (forearm, 0)),
    ]
    sleeve = box(0, 0, upper + 0.23, 0.25, 0.12, SLEEVE, INK)
    lower = box(0, 0, forearm + 0.20, 0.22, 0.10, SLEEVE, INK)
    cuff = box(0, 0, 0.11, 0.23, 0.04, CUFF, INK)
    glove = curve([
        (-0.06, -0.09), (-0.01, -0.12), (0.07, -0.12), (0.10, -0.07),
        (0.16, -0.05), (FINGERTIP, -0.045), (FINGERTIP, 0),
        (FINGERTIP, 0.04), (0.13, 0.043), (0.10, 0.045),
        (0.08, 0.11), (-0.01, 0.12), (-0.06, 0.09),
    ], GLOVE_COLOR, width=1.75)
    return joints, [
        (shoulder, sleeve, (upper / 2, 0)), (elbow, lower, (forearm / 2, 0)),
        (wrist, glove, (0, 0)), (elbow, cuff, (forearm - 0.045, 0)),
    ]


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


def face():
    helmet = box(0, 0.50, 1.17, 0.82, 0.28, SLEEVE, INK)
    visor = box(0, 0.50, 0.88, 0.52, 0.18, INK)
    eyes = [dot(x, 0.54, 0.083, SHEET) for x in (-0.174, 0.174)]
    smile = curve([(-0.066, 0.408), (-0.032, 0.369), (0.032, 0.369), (0.066, 0.408)],
                  edge=APRON, width=1.45)
    stem = curve([(-0.20, 0.865), (-0.23, 0.94), (-0.23, 1.006), (-0.18, 1.057)])
    return VGroup(stem, dot(-0.18, 1.057, 0.060, GLOVE_COLOR), helmet, visor, *eyes, smile)


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
        x_term = formula("x", X_TERM[0], X_TERM[1], 81, VARIABLE_COLOR)
        two_term = formula("2", TWO_TERM[0], TWO_TERM[1], 81, CONSTANT_COLOR)
        original = VGroup(
            formula("3", -5.26, 1.60, 81),
            formula("(", -4.61, 1.57, 81), x_term,
            formula("+", -3.14, 1.54, 81), two_term, formula(")", -1.71, 1.57, 81),
        )
        x_tip, two_tip = point_below(x_term), point_below(two_term)
        x_mark = underline(x_tip, VARIABLE_COLOR)
        two_mark = underline(two_tip, CONSTANT_COLOR)
        center = -3.12
        robot = coat_and_boots().shift((center, 0, 0))
        head = face().shift((center, 0, 0))
        left_joints, left_arm = arm("left", -1)
        right_joints, right_arm = arm("right", 1)
        pupils = [
            ("left_eye", dot(0, 0, 0.034, INK), (0.041, 0)),
            ("right_eye", dot(0, 0, 0.034, INK), (0.041, 0)),
        ]
        joints = [
            Joint2D("guide", offset=(center, 0)),
            *left_joints, *right_joints,
            Joint2D("gaze", "guide", (0, 0.54), PI / 2),
            Joint2D("left_eye", "guide", (-0.174, 0.54), PI / 2),
            Joint2D("right_eye", "guide", (0.174, 0.54), PI / 2),
        ]
        rig = self.retarget_rig_2d(joints=joints, tracks={})
        point_x = rig.look_and_point(
            "left_wrist", "gaze", [(1, x_tip[:2]), (1.1, x_tip[:2])],
            look=(0, 0.24), rest=1.47, fingertip=FINGERTIP,
        )
        point_two = rig.look_and_point(
            "right_wrist", "gaze", [(2, two_tip[:2]), (2.1, two_tip[:2])],
            look=(1.48, 1.64), rest=3, fingertip=FINGERTIP, bend_left=False,
        )
        glance = point_x.pop("gaze") + point_two.pop("gaze")
        phase = self.value_tracker(0)
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
        self.add(paper, sheet, title, subtitle, original, divider, copy_label, shadow,
                 *(part for _, part, _ in left_arm + right_arm),
                 robot, head, *(part for _, part, _ in pupils))
        self.bind_rig_2d(phase, joints=joints,
                         tracks={**point_x, **point_two, "left_eye": glance, "right_eye": glance},
                         attachments=left_arm + right_arm + pupils)
        self.wait(0.45)
        self.play(LaggedStart(FadeIn(cards[0]), FadeIn(cards[1]), FadeIn(cards[2]),
                              lag_ratio=0.30), run_time=0.95)
        self.play(phase.animate(rate_func=linear).set_value(1), Create(x_mark), run_time=0.72)
        self.add(x_panel, *x_tiles)
        self.play(collect_tiles(x_tiles, 0.85), FadeIn(collect_label), run_time=0.85)
        self.play(FadeIn(product_x), run_time=0.27)
        self.wait(0.64)
        self.play(phase.animate(rate_func=linear).set_value(2),
                  FadeOut(x_mark), Create(two_mark), run_time=0.72)
        self.add(two_panel, *two_tiles)
        self.play(collect_tiles(two_tiles, 4.07), FadeIn(plus), run_time=0.85)
        self.play(FadeIn(product_two), run_time=0.27)
        self.wait(0.62)
        self.play(phase.animate(rate_func=linear).set_value(3),
                  FadeOut(two_mark), FadeIn(answer), run_time=0.72)
        self.wait(1.45)
