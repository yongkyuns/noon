"""Use the same joint-local artwork cues for two eyelids on turning heads."""

from noon import *


BLINK = [(0, "open"), (1, "half"), (2, "closed"), (3, "half"), (4, "open"), (5, "open")]
TILT = [(0, -0.16), (1, 0.04), (2, 0.36), (3, 0.24), (4, 0.04), (5, -0.16)]


def aperture(width, height, slant):
    # Both halves keep their Bézier anchors and handles through the blink.
    return [(-width, 0), (-0.48 * width, height * (1 - slant)), (0.48 * width, height * (1 + slant)),
            (width, 0), (0.48 * width, -0.76 * height), (-0.48 * width, -0.76 * height), (-width, 0)]


def eyelid(points, color):
    start, a, b, end, c, d, close = [(x, y, 0) for x, y in points]
    return (VMobject(fill_color=color, fill_opacity=1, stroke_width=0)
            .start_new_path(start).add_cubic_bezier_curve_to(a, b, end)
            .add_cubic_bezier_curve_to(c, d, close).close_path())


def head_pose(shell, eye_offset, color, rig, eye, phase):
    x, y, angle = rig.pose(phase).joint("head")
    visor = RoundedRectangle(width=2.55, height=1.18, corner_radius=0.37,
                             fill="#172539", fill_opacity=1, stroke="#152034", stroke_width=2)
    visor.shift((*eye_offset, 0))
    vent = Line((-0.28, -0.75, 0), (0.28, -0.75, 0), color="#172539", stroke_width=5)
    lights = [Circle(radius=0.06, fill=color, fill_opacity=1, stroke=None)
              .shift((0.18 * i, -1.04, 0)) for i in (-1, 0, 1)]
    housing = VGroup(shell.copy(), visor, vent, *lights)
    housing.rotate(angle, about_point=ORIGIN).shift((x, y, 0))
    return VGroup(housing, eyelid(eye.points_at(phase), color))


class RetargetBlink(Scene):
    def construct(self):
        self.add(Rectangle(width=14.4, height=8.3, fill="#10192B", fill_opacity=1, stroke=None))
        self.add(Text("One blink, two faces", font_size=43, color=color_from_hex("#F5F6ED"))
                 .shift(2.92 * UP))
        self.add(Text("Open, half, and closed poses follow each turning head", font_size=23,
                      color=color_from_hex("#B9C6DE")).shift(2.27 * UP))

        cast = [
            (-3.18, Circle(radius=1.64, fill="#2F726E", fill_opacity=1, stroke="#94E2CD", stroke_width=4),
             (0.24, 0.32), "#A4F6DC", 0.99, 0.59, 0.12, 1.0),
            (3.18, RoundedRectangle(width=3.39, height=3.06, corner_radius=0.58,
                                    fill="#7A4A40", fill_opacity=1, stroke="#EAB389", stroke_width=4),
             (-0.35, 0.34), "#FFD29C", 0.83, 0.71, -0.30, -0.83),
        ]
        actors = []
        for x, shell, offset, color, width, height, slant, tilt_gain in cast:
            rig = self.retarget_rig_2d(
                joints=[Joint2D("head", offset=(x, -0.26)), Joint2D("eye", parent="head", offset=offset)],
                tracks={"head": TILT},
                mapping={"head": ("head", tilt_gain)},
            )
            poses = {name: aperture(width, height * opening, slant)
                     for name, opening in (("open", 1), ("half", 0.44), ("closed", 0.075))}
            eye = rig.artwork_track("eye", poses=poses, cues=BLINK)
            appearance = (shell, offset, color, rig, eye)
            face = head_pose(*appearance, 0)
            self.add(face)
            actors.append((face, appearance))

        def turn_to(phase):
            return [Transform(face, head_pose(*appearance, phase)) for face, appearance in actors]

        self.wait(0.5)
        self.play(*turn_to(1), run_time=0.75)
        self.play(*turn_to(2), run_time=0.75)
        self.wait(0.5)
        for phase in (3, 4, 5):
            self.play(*turn_to(phase), run_time=0.75)
        self.wait(0.5)
