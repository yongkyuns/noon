"""Two differently proportioned characters follow a label using scene-space cues."""

from noon import *


PAPER = color_from_hex("#F5ECD8")
SHEET = color_from_hex("#FFFAEF")
INK = color_from_hex("#293643")
RULE = color_from_hex("#D8C9AE")
TEAL = color_from_hex("#30756D")
PLUM = color_from_hex("#894E68")
GLOVE_COLOR = color_from_hex("#EBAC4C")

LABEL_START = (-0.42, -0.34)
LABEL_END = (0.40, 0.82)
LABEL_EDGE = 0.98
FINGERTIP = 0.34


def box(width, height, radius, fill, center=(0, 0, 0), stroke=None):
    return RoundedRectangle(width=width, height=height, corner_radius=radius,
                            fill=fill, fill_opacity=1, stroke=stroke, stroke_width=1.8).move_to(center)


def disc(radius, fill, center=(0, 0, 0), stroke=None):
    return Circle(radius=radius, fill=fill, fill_opacity=1,
                  stroke=stroke, stroke_width=2).move_to(center)


def arm(shoulder, lengths, color, side, eye_center):
    upper, forearm = lengths
    shoulder_angle, elbow_angle = (-0.55, 3.45) if side < 0 else (-2.78, 2.98)
    joints = [
        Joint2D("body"),
        Joint2D("shoulder", "body", shoulder, shoulder_angle),
        Joint2D("elbow", "shoulder", (upper, 0), elbow_angle),
        Joint2D("wrist", "elbow", (forearm, 0)),
        Joint2D("gaze", "body", eye_center, PI / 2),
        Joint2D("right_eye", "body", (eye_center[0] + 0.32, eye_center[1]), PI / 2),
    ]
    sleeve = box(upper + 0.18, 0.31, 0.14, color, stroke=INK)
    cuff = box(forearm + 0.12, 0.23, 0.10, color, stroke=INK)
    glove = Polygon((-0.11, -0.085, 0), (0.12, -0.13, 0), (0.20, -0.06, 0),
                    (FINGERTIP, -0.025, 0), (FINGERTIP, 0.025, 0), (0.19, 0.07, 0),
                    (0.10, 0.13, 0), (-0.11, 0.085, 0), color=GLOVE_COLOR)
    glove.set_fill(GLOVE_COLOR, opacity=1).set_stroke(INK, width=1.7)
    shoulder_pin = disc(0.16, SHEET, stroke=INK)
    elbow_pin = disc(0.115, SHEET, stroke=INK)
    return joints, [
        ("shoulder", sleeve, (upper / 2, 0)),
        ("elbow", cuff, (forearm / 2, 0)),
        ("wrist", glove, (0, 0)),
        ("shoulder", shoulder_pin, (0, 0)),
        ("elbow", elbow_pin, (0, 0)),
        ("gaze", disc(0.028, INK), (0.038, 0)),
        ("right_eye", disc(0.028, INK), (0.038, 0)),
    ]


def puppet(x, color, side, tall):
    body_width, body_height = (1.04, 1.65) if tall else (1.40, 1.1)
    body_y = -2.37 + body_height / 2
    head_y = body_y + body_height / 2 + 0.31
    feet = [box(0.40, 0.24, 0.10, INK, (x + dx, -2.40, 0)) for dx in (-0.28, 0.28)]
    body = box(body_width, body_height, 0.25, color, (x, body_y, 0), INK)
    panel = box(0.49, 0.32, 0.11, SHEET, (x, body_y - 0.14, 0))
    bolts = [disc(0.041, color, (x + dx, body_y - 0.14, 0)) for dx in (-0.12, 0.12)]
    head = box(body_width + 0.18, 0.80, 0.32 if tall else 0.24, color, (x, head_y, 0), INK)
    visor_x = x - side * 0.07
    visor = box(body_width - 0.16, 0.43, 0.16, INK, (visor_x, head_y, 0))
    eyes = [disc(0.071, SHEET, (visor_x + dx, head_y + 0.026, 0)) for dx in (-0.16, 0.16)]
    return [*feet, body, panel, *bolts, head, visor, *eyes], (visor_x - 0.16, head_y + 0.026)


class TargetPointing(Scene):
    def construct(self):
        self.add(Rectangle(width=14.4, height=8.2, fill=PAPER, fill_opacity=1, stroke=None),
                 box(13.45, 7.02, 0.32, SHEET, stroke=RULE),
                 Text("Follow the label", font="Libertinus Serif", font_size=59, color=INK).shift(2.70 * UP),
                 Text("Same cue, different arm lengths", font="Arimo", font_size=25, color=INK).shift(2.04 * UP),
                 Line((-5.66, -2.52, 0), (5.66, -2.52, 0), stroke_color=RULE, stroke_width=1.8))
        phase = self.value_tracker(0)
        # One path drives the label and supplies its edge cues to both rigs.
        outward = [(t / 24, LABEL_START[0] + (LABEL_END[0] - LABEL_START[0]) * t / 24,
                    LABEL_START[1] + (LABEL_END[1] - LABEL_START[1]) * t / 24)
                   for t in range(25)]
        path = ([(1 + t, x, y) for t, x, y in outward]
                + [(3 - t, x, y) for t, x, y in reversed(outward[:-1])])
        for x, shoulder, lengths, color, side, tall in [
            (-4.18, (-3.66, -1.00), (1.70, 1.73), TEAL, -1, True),
            (4.22, (3.52, -1.50), (2.02, 1.14), PLUM, 1, False),
        ]:
            body, eye_center = puppet(x, color, side, tall)
            joints, attachments = arm(shoulder, lengths, color, side, eye_center)
            rig = self.retarget_rig_2d(joints=joints, tracks={})
            edge = [(t, (x + side * LABEL_EDGE, y)) for t, x, y in path]
            tracks = rig.look_and_point("wrist", "gaze", edge, look=(0, 0.23), rest=4,
                                       fingertip=FINGERTIP, bend_left=side > 0)
            self.add(*body, *(part for _, part, _ in attachments))
            self.bind_rig_2d(phase, joints=joints, tracks={**tracks, "right_eye": tracks["gaze"]},
                             attachments=attachments)
        label = [
            (box(1.96, 0.68, 0.15, INK), 0),
            (Text("LOOK HERE", font="Arimo", font_size=20, color=SHEET), 0),
            (disc(0.068, SHEET, stroke=INK), -LABEL_EDGE),
            (disc(0.068, SHEET, stroke=INK), LABEL_EDGE),
        ]
        for part, offset in label:
            self.add(part)
            self.bind_sampled(part, phase, "position", [(t, x + offset, y) for t, x, y in path])
        self.play(phase.animate(rate_func=linear).set_value(1), run_time=1.1)
        self.play(phase.animate.set_value(2), run_time=2.0)
        self.wait(0.8)
        self.play(phase.animate.set_value(3), run_time=1.2)
        self.play(phase.animate(rate_func=linear).set_value(4), run_time=0.55)
        self.wait(0.7)
