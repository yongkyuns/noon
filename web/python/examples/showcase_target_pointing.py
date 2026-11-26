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


def arm(scene, phase, shoulder, lengths, color, side, path):
    upper, forearm = lengths
    joints = [
        Joint2D("shoulder", offset=shoulder),
        Joint2D("elbow", "shoulder", (upper, 0)),
        Joint2D("wrist", "elbow", (forearm, 0)),
    ]
    rig = scene.retarget_rig_2d(joints=joints, tracks={})
    edge = [(t, (x + side * LABEL_EDGE, y)) for t, x, y in path]
    tracks = rig.pointing_tracks("wrist", edge, fingertip=FINGERTIP, bend_left=side > 0)
    sleeve = box(upper + 0.18, 0.31, 0.14, color, stroke=INK)
    cuff = box(forearm + 0.12, 0.23, 0.10, color, stroke=INK)
    glove = Polygon((-0.11, -0.085, 0), (0.12, -0.13, 0), (0.20, -0.06, 0),
                    (FINGERTIP, -0.025, 0), (FINGERTIP, 0.025, 0), (0.19, 0.07, 0),
                    (0.10, 0.13, 0), (-0.11, 0.085, 0),
                    fill=GLOVE_COLOR, fill_opacity=1, stroke=INK, stroke_width=1.7)
    shoulder_pin = disc(0.16, SHEET, stroke=INK)
    elbow_pin = disc(0.115, SHEET, stroke=INK)
    scene.add(sleeve, cuff, glove, shoulder_pin, elbow_pin)
    scene.bind_rig_2d(phase, joints=joints, tracks=tracks, attachments=[
        ("shoulder", sleeve, (upper / 2, 0)), ("shoulder", shoulder_pin, (0, 0)),
        ("elbow", cuff, (forearm / 2, 0)), ("elbow", elbow_pin, (0, 0)),
        ("wrist", glove, (0, 0)),
    ])


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
    return [*feet, body, panel, *bolts, head, visor, *eyes]


class TargetPointing(Scene):
    def construct(self):
        self.add(Rectangle(width=14.4, height=8.2, fill=PAPER, fill_opacity=1, stroke=None),
                 box(13.45, 7.02, 0.32, SHEET, stroke=RULE),
                 Text("Follow the label", font="Libertinus Serif", font_size=59, color=INK).shift(2.70 * UP),
                 Text("Same cue, different arm lengths", font="Arimo", font_size=25, color=INK).shift(2.04 * UP),
                 Line((-5.66, -2.52, 0), (5.66, -2.52, 0), stroke_color=RULE, stroke_width=1.8))
        self.add(*puppet(-4.18, TEAL, -1, True), *puppet(4.22, PLUM, 1, False))
        phase = self.value_tracker(0)
        # One path drives the label and supplies its edge cues to both rigs.
        path = [(t / 24, LABEL_START[0] + (LABEL_END[0] - LABEL_START[0]) * t / 24,
                 LABEL_START[1] + (LABEL_END[1] - LABEL_START[1]) * t / 24)
                for t in range(25)]
        arm(self, phase, (-3.66, -1.00), (1.70, 1.73), TEAL, -1, path)
        arm(self, phase, (3.52, -1.50), (2.02, 1.14), PLUM, 1, path)
        label = [
            (box(1.96, 0.68, 0.15, INK), 0),
            (Text("LOOK HERE", font="Arimo", font_size=25, color=SHEET), 0),
            (disc(0.068, SHEET, stroke=INK), -LABEL_EDGE),
            (disc(0.068, SHEET, stroke=INK), LABEL_EDGE),
        ]
        for part, offset in label:
            self.add(part)
            self.bind_sampled(part, phase, "position", [(t, x + offset, y) for t, x, y in path])
        self.wait(1.1)
        self.play(phase.animate.set_value(1), run_time=2.0)
        self.wait(0.8)
        self.play(phase.animate.set_value(0), run_time=1.2)
        self.wait(1.25)
