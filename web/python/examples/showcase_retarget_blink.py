"""Use the same joint-local artwork cues for two eyelids on turning heads."""

from noon import *


BLINK = [(0, "open"), (2, "closed"), (4, "open")]
TILT = [(0, -0.13), (1, 0.04), (2, 0.27), (3, 0.19), (4, 0.04), (5, -0.13)]
EYE_COLORS = ("#FFFCF4", "#32465A", "#FFFFFF") * 2


def eye_poses(oval, spacing, width, height):
    def place(layers):
        # Every layer uses the same ellipse points, so the poses correspond.
        return [
            (eye_x + dx + sx * x, dy + sy * y + bow * (x * x - 1))
            for eye_x in (-spacing, spacing)
            for dx, dy, sx, sy, bow in layers
            for x, y in oval
        ]

    return {
        "open": place([(0, 0, width, height, 0), (0.035, 0, 0.48 * width, 0.73 * height, 0),
                       (0, 0.24 * height, 0.14 * width, 0.19 * height, 0)]),
        "closed": place([(0, 0, 0.84 * width, 0.020, 0.095), (0, 0, 0.84 * width, 0.024, 0.095),
                         (0, -0.095, 0.005, 0.002, 0)]),
    }


def face_art(shell, blush):
    smile = ArcBetweenPoints((-0.26, -0.48, 0), (0.26, -0.48, 0), angle=PI / 2,
                             stroke_color="#3C5362", stroke_width=4)
    cheeks = [Ellipse(width=0.39, height=0.19, fill=blush, fill_opacity=0.8, stroke=None)
              .shift((side * 0.96, -0.27, 0)) for side in (-1, 1)]
    return VGroup(shell, smile, *cheeks)


def face_at(art, rig, eye, phase, contour_size):
    x, y, angle = rig.pose(phase).joint("head")
    head = art.copy().rotate(angle, about_point=ORIGIN).shift((x, y, 0))
    points = eye.points_at(phase)
    eyes = [VMobject(fill_color=color, fill_opacity=1, stroke_width=0)
            .set_points([(x, y, 0) for x, y in points[i:i + contour_size]])
            for i, color in zip(range(0, len(points), contour_size), EYE_COLORS)]
    return VGroup(head, *eyes)


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
        oval = [(x, y) for x, y, _ in Ellipse(width=2, height=2).get_all_points()]
        actors = []
        for x, shell, offset, cheek_color, spacing, width, height, tilt_gain in cast:
            rig = self.retarget_rig_2d(
                joints=[Joint2D("head", offset=(x, -0.26)), Joint2D("eye", parent="head", offset=offset)],
                tracks={"head": TILT},
                mapping={"head": ("head", tilt_gain)},
            )
            eye = rig.artwork_track("eye", poses=eye_poses(oval, spacing, width, height), cues=BLINK)
            art = face_art(shell, cheek_color)
            face = face_at(art, rig, eye, 0, len(oval))
            self.add(face)
            actors.append((face, art, rig, eye))

        def turn_to(phase):
            return [Transform(face, face_at(art, rig, eye, phase, len(oval)))
                    for face, art, rig, eye in actors]

        self.wait(0.5)
        self.play(*turn_to(1), run_time=0.75)
        self.play(*turn_to(2), run_time=0.75)
        self.wait(0.5)
        for phase in (3, 4, 5):
            self.play(*turn_to(phase), run_time=0.75)
        self.wait(0.5)
