"""Fold two different arms with one clip. Author the hand views in scene code."""

from math import cos, sin

from noon import *


PAPER = "#F7ECD7"
INK = "#303047"
HAND = "#FFD075"
PALM = "#B35A40"
HIGHLIGHT = "#F7EDC7"
# Each phase has a shoulder angle, an elbow angle, and an authored hand view.
GESTURE = (
    (0, 0, 0, "palm"),
    (0.35, 0.13, 0.72, "palm"),
    (0.55, 0.24, 1.72, "edge"),
    (0.80, 0.39, 2.90, "back"),
    (1, 0, 0, "palm"),
)


def path(points, fill, edge=INK, width=2.0, closed=True):
    start, *handles = [(x, y, 0) for x, y in points]
    drawing = VMobject(fill_color=fill, fill_opacity=1 if closed else 0,
                       stroke_color=edge, stroke_width=width).start_new_path(start)
    for index in range(0, len(handles), 3):
        drawing.add_cubic_bezier_curve_to(*handles[index:index+3])
    return drawing.close_path() if closed else drawing


def ink_line(start, end, color=INK, width=1.3):
    return Line((*start, 0), (*end, 0), stroke_color=color, stroke_width=width)


def dot(radius, x, y, color):
    return Circle(radius=radius, fill=color, fill_opacity=1, stroke=None).move_to((x, y, 0))


def torso(x, coat, seam):
    jacket = RoundedRectangle(width=1.65, height=1.73, corner_radius=0.29,
                              fill_color=coat, fill_opacity=1, stroke_color=INK, stroke_width=2.2)
    jacket.move_to((x, 0.02, 0))
    collar = path([
        (x-0.71, 0.70), (x-0.54, 0.78), (x-0.22, 0.68), (x, 0.44),
        (x+0.24, 0.70), (x+0.54, 0.78), (x+0.70, 0.70),
        (x+0.55, 0.46), (x+0.28, 0.37), (x, 0.31),
        (x-0.26, 0.36), (x-0.55, 0.46), (x-0.71, 0.70),
    ], seam, width=1.35)
    details = [jacket, collar, ink_line((x, -0.80), (x, 0.31), seam, 2.5)]
    details.extend(dot(0.045, x-0.13, y, HIGHLIGHT) for y in (0.08, -0.24, -0.56))
    return VGroup(*details)


def head(x, coat, shade):
    cap = RoundedRectangle(width=1.44, height=0.99, corner_radius=0.38,
                           fill_color=coat, fill_opacity=1, stroke_color=INK, stroke_width=2.2)
    cap.move_to((x, 1.54, 0))
    visor = RoundedRectangle(width=1.08, height=0.56, corner_radius=0.22,
                             fill_color=INK, fill_opacity=1, stroke_width=0).move_to((x, 1.52, 0))
    return VGroup(cap, visor,
                  ink_line((x-0.47, 2.14), (x-0.47, 1.89), INK, 2.0),
                  dot(0.073, x-0.47, 2.17, HAND),
                  dot(0.081, x-0.19, 1.54, HIGHLIGHT), dot(0.081, x+0.23, 1.54, HIGHLIGHT),
                  ink_line((x-0.04, 1.37), (x+0.09, 1.37), shade, 1.35))


def hand_points(view, place):
    # Equal cubic topology for all three drawings. The lip at the thumb side
    # flattens at the edge view, then reappears on the opposite side.
    top, bottom = {"palm": (0.15, -0.26), "edge": (0.055, -0.055), "back": (0.26, -0.15)}[view]
    return [place(x, y) for x, y in [
        (-0.05, 0), (-0.05, 0.83*top), (0.08, top), (0.15, top),
        (0.19, top), (0.25, top*0.63), (0.31, top*0.71),
        (0.50, top*0.85), (0.59, top*0.52), (0.59, 0),
        (0.59, bottom*0.52), (0.50, bottom*0.85), (0.31, bottom*0.71),
        (0.25, bottom*0.63), (0.19, bottom), (0.15, bottom),
        (0.08, bottom), (-0.05, 0.83*bottom), (-0.05, 0),
    ]]


def placement(joint):
    x, y, heading = joint
    c, s = cos(heading), sin(heading)
    return lambda a, b: (x + c*a - s*b, y + s*a + c*b)


def arm_drawing(rig, key, sleeve_color):
    phase, _, _, view = GESTURE[key]
    pose = rig.pose(phase)
    # Rebuild one sleeve through the actual joints. The two rounded
    # strokes share a centerline, including where the folded arm overlaps.
    joints = [(*pose.joint(name)[:2], 0) for name in ("shoulder", "elbow", "wrist")]
    def sleeve(color, width):
        return VMobject(fill_opacity=0, stroke_color=color, stroke_width=width,
                        stroke_join="round", stroke_cap="round").set_points_as_corners(joints)

    place = placement(pose.joint("wrist"))
    glove = path(hand_points(view, place), HAND, width=1.8)
    # The interior marks tell palm from knuckles. Their opacity collapses
    # through the thin edge view while the same glove outline keeps moving.
    crease = path([place(0.16, -0.13), place(0.20, -0.06),
                   place(0.32, 0.03), place(0.38, 0.025)],
                  None, PALM, width=1.6, closed=False)
    crease.set_stroke(opacity=1 if view == "palm" else 0)
    knuckles = [ink_line(place(0.44, y), place(0.53, y*0.85), PALM, 1.35)
                .set_stroke(opacity=1 if view == "back" else 0) for y in (-0.069, 0.069)]
    return VGroup(sleeve(INK, 33), sleeve(sleeve_color, 29), glove, crease, *knuckles)


class RetargetHand(Scene):
    def _arm_rig(self, x, upper, forearm):
        return self.retarget_rig_2d(
            joints=[
                Joint2D("shoulder", offset=(x + 0.46, 0.47), rest_angle=-0.25),
                Joint2D("elbow", parent="shoulder", offset=(upper, 0)),
                Joint2D("wrist", parent="elbow", offset=(forearm, 0)),
            ],
            tracks={
                "shoulder": [(phase, shoulder) for phase, shoulder, _, _ in GESTURE],
                "elbow": [(phase, elbow) for phase, _, elbow, _ in GESTURE],
            },
        )

    def construct(self):
        self.add(Rectangle(width=14.4, height=8.3, fill=PAPER, fill_opacity=1, stroke=None))
        self.add(Text("One fold, two reaches", font_size=39, color=color_from_hex(INK))
                 .move_to((0, 3.02, 0)))
        self.add(ink_line((0, -1.65), (0, 2.25), "#D5C4A4", 1.8))
        actors = []
        for x, upper, forearm, coat, seam, sleeve, front in [
            (-3.76, 0.76, 1.27, "#4B8279", "#2B585D", "#78AA93", True),
            (2.20, 0.57, 1.13, "#B95F56", "#873F47", "#DB8875", False),
        ]:
            rig = self._arm_rig(x, upper, forearm)
            arm = arm_drawing(rig, 0, sleeve)
            body = torso(x, coat, seam)
            self.add(*((body, arm) if front else (arm, body)))
            self.add(head(x, coat, sleeve))
            self.add(Text("Hand in front" if front else "Hand behind", font_size=27,
                          color=color_from_hex(INK)).move_to((x + 0.48, -1.59, 0)))
            actors.append((rig, arm, sleeve))

        def turn_to(key):
            return [Transform(arm, arm_drawing(rig, key, color), run_time=0.75, rate_func=smooth)
                    for rig, arm, color in actors]

        self.wait(0.75)
        self.play(*turn_to(1))
        self.play(*turn_to(2))
        self.wait(0.25)
        self.play(*turn_to(3))
        self.wait(0.75)
        self.play(*turn_to(2))
        self.play(*turn_to(1))
        self.play(*turn_to(4))
        self.wait(0.5)
