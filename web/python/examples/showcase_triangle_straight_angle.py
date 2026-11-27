"""Three triangle corners become one straight angle."""

from math import atan2, cos, sin
from noon import *


PAPER = color_from_hex("#E9EFEA")
BOARD = color_from_hex("#FAFCF8")
INK = color_from_hex("#1C333A")
MUTED = color_from_hex("#52686B")
RULE = color_from_hex("#D4E0DB")
PROOF_PAPER = color_from_hex("#F0F4EB")
TEAL = color_from_hex("#29786F")
ENAMEL = color_from_hex("#77AEA3")
PALE = color_from_hex("#D6E9DC")
GLASS = color_from_hex("#EEF8ED")
GLOVE = color_from_hex("#F4BD58")
ANGLE_COLORS = (
    color_from_hex("#32857B"), color_from_hex("#90618D"), color_from_hex("#C76A50"),
)

VERTICES = ((-2.20, -0.16), (0.90, -0.16), (-0.60, 1.80))
LABEL_OFFSETS = ((-0.22, 0.22), (0.22, 0.22), (0, 0.21))
PROOF_CENTER = (4.08, -0.52)
RADIUS = 1.08
FINGERTIP = 0.27


def box(center, width, height, radius, color, outline=None):
    return RoundedRectangle(width=width, height=height, corner_radius=radius,
                            fill=color, fill_opacity=1, stroke=outline,
                            stroke_width=1.9 if outline else 0).move_to((*center, 0))


def dot(center, radius, color):
    return Circle(radius=radius, fill=color, fill_opacity=1, stroke=None).move_to((*center, 0))


def label(message, center, size, color=INK, font="Arimo"):
    return Text(message, font=font, font_size=size, color=color).move_to((*center, 0))


def corner_angles(vertices):
    corners = []
    for index, (x, y) in enumerate(vertices):
        nx, ny = vertices[(index + 1) % 3]
        px, py = vertices[(index - 1) % 3]
        start, end = atan2(ny - y, nx - x), atan2(py - y, px - x)
        sweep = (end - start) % TAU
        corners.append((start, sweep) if sweep <= PI else (end, TAU - sweep))
    return corners


def sector(origin, start, sweep, color):
    return Sector(radius=RADIUS, angle=sweep, start_angle=start,
                  arc_center=origin, fill_opacity=1, stroke_width=0, color=color)


def guide_body():
    return VGroup(
        Ellipse(width=1.92, height=0.20, fill=RULE, fill_opacity=1,
                stroke=None).move_to((-4.29, -2.46, 0)),
        box((-4.59, -1.97), 0.20, 0.47, 0.08, MUTED),
        box((-4.05, -1.97), 0.20, 0.47, 0.08, MUTED),
        box((-4.58, -2.29), 0.51, 0.29, 0.11, INK),
        box((-4.03, -2.29), 0.51, 0.29, 0.11, INK),
        box((-4.28, -1.08), 1.46, 1.52, 0.31, TEAL, INK),
        box((-4.40, -1.02), 0.72, 0.74, 0.14, PALE),
        Line((-4.62, -1.13, 0), (-4.20, -1.13, 0), stroke_color=ENAMEL, stroke_width=2.0),
        Line((-4.62, -0.99, 0), (-4.28, -0.99, 0), stroke_color=ENAMEL, stroke_width=2.0),
        dot((-4.58, -0.80), 0.048, TEAL),
        dot((-4.42, -0.80), 0.048, GLOVE),
        box((-4.26, -0.29), 0.46, 0.27, 0.08, MUTED),
        Line((-4.58, 0.43, 0), (-4.73, 0.79, 0), stroke_color=INK, stroke_width=2.2),
        dot((-4.73, 0.79), 0.058, GLOVE),
        box((-4.29, 0.15), 1.41, 0.91, 0.24, ENAMEL, INK),
        box((-4.21, 0.17), 1.05, 0.58, 0.18, INK),
        dot((-4.07, 0.19), 0.177, PALE),
        dot((-4.07, 0.19), 0.142, GLASS),
        dot((-4.56, 0.18), 0.057, ENAMEL),
    )


def guide_arm():
    upper, forearm = 2.14, 2.24
    joints = [
        Joint2D("guide"),
        Joint2D("shoulder", "guide", (-3.60, -0.29), -0.99),
        Joint2D("elbow", "shoulder", (upper, 0), 2.91),
        Joint2D("wrist", "elbow", (forearm, 0)),
        Joint2D("gaze", "guide", (-4.07, 0.19)),
    ]
    glove = Polygon(
        (-0.14, -0.095, 0), (0.055, -0.10, 0), (0.15, -0.045, 0),
        (FINGERTIP, -0.021, 0), (FINGERTIP, 0.031, 0), (0.13, 0.058, 0),
        (0.06, 0.13, 0), (-0.14, 0.09, 0), color=GLOVE,
    ).set_fill(GLOVE, opacity=1).set_stroke(INK, width=1.35).set_z_index(2)
    attachments = [
        ("shoulder", box((0, 0), upper + 0.18, 0.27, 0.13, TEAL, INK), (upper / 2, 0)),
        ("elbow", box((0, 0), forearm + 0.09, 0.21, 0.10, ENAMEL, INK), (forearm / 2, 0)),
        ("shoulder", dot((0, 0), 0.133, INK), (0, 0)),
        ("shoulder", dot((0, 0), 0.075, PALE), (0, 0)),
        ("elbow", dot((0, 0), 0.119, INK), (0, 0)),
        ("elbow", dot((0, 0), 0.063, PALE), (0, 0)),
        ("wrist", box((0, 0), 0.18, 0.214, 0.065, TEAL, INK), (-0.11, 0)),
        ("wrist", glove, (0, 0)),
        ("gaze", dot((0, 0), 0.064, INK), (0.067, 0)),
        ("gaze", dot((0, 0), 0.021, GLASS), (0.086, 0.019)),
    ]
    return joints, attachments


class TriangleStraightAngle(Scene):
    def construct(self):
        # Vertices drive the drawing, the pointing targets, and the proof.
        angles = corner_angles(VERTICES)
        vertices = [(*vertex, 0) for vertex in VERTICES]
        triangle = Polygon(*vertices, color=INK).set_fill(PALE, opacity=0.30).set_stroke(INK, width=2.8)
        self.add(
            Rectangle(width=14.4, height=8.2, fill=PAPER, fill_opacity=1, stroke=None),
            box((0, 0), 13.36, 7.15, 0.30, BOARD, RULE),
            label("A triangle makes a straight angle", (-0.46, 2.92), 53, font="Libertinus Serif"),
            label("Pivot each interior angle onto one line.", (-0.46, 2.39), 24, MUTED),
            box((4.05, -0.31), 4.40, 4.74, 0.25, PROOF_PAPER),
            label("Put the angles together", (4.05, 1.51), 27, font="Libertinus Serif"),
            sector(PROOF_CENTER, 0, PI, RULE),
            Line((2.50, -0.52, 0), (5.66, -0.52, 0), stroke_color=MUTED, stroke_width=1.6),
            Line((-5.83, -2.58, 0), (1.42, -2.58, 0), stroke_color=RULE, stroke_width=1.6),
            triangle, guide_body(),
        )
        for name, (x, y), (dx, dy), color in zip("ABC", VERTICES, LABEL_OFFSETS, ANGLE_COLORS):
            self.add(label(name, (x + dx, y + dy), 26, color).set_z_index(3),
                     dot((x, y), 0.047, INK).set_z_index(3))

        joints, attachments = guide_arm()
        rig = self.retarget_rig_2d(joints=joints, tracks={})
        cues = [
            {"wrist": "wrist", "targets": [(index + 0.65, vertex)],
             "look": (index, index + 0.14), "rest": index + 1, "bend_left": False}
            for index, vertex in enumerate(VERTICES)
        ]
        tracks = rig.look_and_point_sequence("gaze", *cues, fingertip=FINGERTIP)
        phase = self.value_tracker(0)
        self.add(*(part for _, part, _ in attachments))
        self.bind_rig_2d(phase, joints=joints, tracks=tracks, attachments=attachments)

        wedges = [sector(origin, start, sweep, color)
                  for origin, (start, sweep), color in zip(VERTICES, angles, ANGLE_COLORS)]
        # Place each copy's transform origin at its vertex so the sweep stays fixed.
        assembly = self.value_tracker(0)
        right_edge = PI
        names = []
        for index, (name, (x, y), (start, sweep), color) in enumerate(
                zip("ABC", VERTICES, angles, ANGLE_COLORS)):
            right_edge -= sweep
            turn = (right_edge - start + PI) % TAU - PI
            piece = sector((0, 0), start, sweep, color)
            self.add(piece)
            departure = index * 0.12
            arrival = departure + 0.76
            self.bind_sampled(piece, assembly, "opacity", [(departure, 0), (departure + 0.03, 1)])
            self.bind_sampled(piece, assembly, "rotation", [(departure, 0), (arrival, turn)])
            self.bind_sampled(piece, assembly, "position",
                              [(departure, x, y), (arrival, *PROOF_CENTER)])
            mid = right_edge + sweep / 2
            names.append(label(name, (PROOF_CENTER[0] + 0.73 * cos(mid),
                                      PROOF_CENTER[1] + 0.73 * sin(mid)), 25, BOARD))
        total = MathTypst("angle A + angle B + angle C = 180°", font_size=36,
                          color=INK).move_to((4.08, -1.47, 0))

        self.wait(0.50)
        for index, wedge in enumerate(wedges):
            self.play(phase.animate(rate_func=linear).set_value(index + 0.65),
                      FadeIn(wedge), run_time=0.65)
            self.wait(0.45)
            self.play(phase.animate(rate_func=linear).set_value(index + 1), run_time=0.40)
        self.play(assembly.animate(rate_func=linear).set_value(1), run_time=1.90)
        self.play(FadeIn(VGroup(*names, total)), run_time=0.40)
        self.wait(1.40)
