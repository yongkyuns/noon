"""Use the same joint-local artwork cues for two eyelids on turning heads."""

from noon import *


BLINK = [(0, "open"), (1, "half"), (2, "closed"), (3, "half"), (4, "open"), (5, "open")]
TILT = [(0, -0.13), (1, 0.04), (2, 0.27), (3, 0.19), (4, 0.04), (5, -0.13)]
EYE_COLORS = ("#FFFCF4", "#32465A", "#FFFFFF") * 2


def oval_points(x, y, width, height):
    tip = 4 * height / 3
    left, right = x - width, x + width
    return [(left, y), (left, y + tip), (right, y + tip), (right, y),
            (right, y - tip), (left, y - tip), (left, y)]


def eye_points(spacing, width, height, opening):
    # The whites, pupils, and highlights keep the same paths in every pose.
    return [point for x in (-spacing, spacing) for contour in (
        oval_points(x, 0, width, height * opening),
        oval_points(x + 0.035, 0, width * 0.48, height * opening * 0.73),
        oval_points(x, height * opening * 0.24, width * 0.14, height * opening * 0.19),
    ) for point in contour]


def eye_path(points, color):
    start, a, b, end, c, d, close = [(x, y, 0) for x, y in points]
    return (VMobject(fill_color=color, fill_opacity=1, stroke_width=0)
            .start_new_path(start).add_cubic_bezier_curve_to(a, b, end)
            .add_cubic_bezier_curve_to(c, d, close).close_path())


def head_pose(shell, cheek_color, rig, eye, phase):
    x, y, angle = rig.pose(phase).joint("head")
    smile = (VMobject(stroke_color="#3C5362", stroke_width=4, fill_opacity=0)
             .start_new_path((-0.26, -0.48, 0))
             .add_cubic_bezier_curve_to((-0.15, -0.74, 0), (0.15, -0.74, 0), (0.26, -0.48, 0)))
    cheeks = [Ellipse(width=0.39, height=0.19, fill=cheek_color, fill_opacity=0.8, stroke=None)
              .shift((side * 0.96, -0.27, 0)) for side in (-1, 1)]
    face = VGroup(shell.copy(), smile, *cheeks)
    face.rotate(angle, about_point=ORIGIN).shift((x, y, 0))
    points = eye.points_at(phase)
    return VGroup(face, *(eye_path(points[i:i + 7], color)
                          for i, color in zip(range(0, len(points), 7), EYE_COLORS)))


class RetargetBlink(Scene):
    def construct(self):
        self.add(Rectangle(width=14.4, height=8.3, fill="#FFF5E7", fill_opacity=1, stroke=None))
        self.add(Text("One blink, two faces", font_size=43, color=color_from_hex("#28384A"))
                 .shift(2.92 * UP))
        self.add(Text("Open, half, and closed poses follow each turning head", font_size=23,
                      color=color_from_hex("#556579")).shift(2.27 * UP))

        cast = [
            (-3.0, Circle(radius=1.64, fill="#B5E3D2", fill_opacity=1, stroke="#5C9F8D", stroke_width=4),
             (0.11, 0.32), "#F1ACA1", 0.41, 0.31, 0.44, 1.0),
            (3.0, RoundedRectangle(width=3.39, height=3.06, corner_radius=0.79,
                                   fill="#FFD0AD", fill_opacity=1, stroke="#C18267", stroke_width=4),
             (-0.10, 0.26), "#EB9B97", 0.48, 0.35, 0.36, -0.83),
        ]
        actors = []
        for x, shell, offset, cheek_color, spacing, width, height, tilt_gain in cast:
            rig = self.retarget_rig_2d(
                joints=[Joint2D("head", offset=(x, -0.26)), Joint2D("eye", parent="head", offset=offset)],
                tracks={"head": TILT},
                mapping={"head": ("head", tilt_gain)},
            )
            poses = {name: eye_points(spacing, width, height, opening)
                     for name, opening in (("open", 1), ("half", 0.53), ("closed", 0.065))}
            eye = rig.artwork_track("eye", poses=poses, cues=BLINK)
            appearance = (shell, cheek_color, rig, eye)
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
