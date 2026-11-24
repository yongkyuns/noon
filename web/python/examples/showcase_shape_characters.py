"""Retarget the paper birds' joint clip to deform three continuous outlines."""

from noon import *


# Identical source keys from the paper birds, kept inline for Playground use.
SOURCE_CLIP = {
    "shoulder": [(0, 0), (0.96, 0), (1.27, -0.08), (1.91, 0.95), (3.15, 0.95),
                 (3.48, 0.82), (3.96, 0.97), (5.03, 0.97), (5.65, 0)],
    "wrist": [(0, 0), (1.66, 0), (2.70, 2.52), (3.15, 2.52), (3.54, 2.20),
              (3.96, 2.52), (4.32, 2.52), (5.25, 0)],
}
POSE_TIMES = (1.27, 1.91, 2.70, 3.15, 3.54, 3.96, 4.32, 5.25)
REST_TIME = 5.65
INK = "#303047"
CREAM = "#FFF5DD"
FLOOR = -2.30


def oval(width, height, x, y, fill, border=None):
    shape = Ellipse(width=width, height=height, fill=fill, fill_opacity=1, stroke=border)
    if border is not None:
        shape.set_stroke(border, width=1.65)
    return shape.move_to((x, y, 0))


def stroke(points, color, width):
    start, a, b, end = [(x, y, 0) for x, y in points]
    return (VMobject(stroke_color=color, stroke_width=width, fill_opacity=0)
            .start_new_path(start).add_cubic_bezier_curve_to(a, b, end))


def mouth(amount):
    width = 0.17 - 0.07 * amount
    y = -0.24
    top = y - 0.028 + 0.13 * amount
    low = y - 0.10 - 0.08 * amount
    return (VMobject(fill_color=INK, fill_opacity=1, stroke_width=0)
            .start_new_path((-width, y, 0))
            .add_cubic_bezier_curve_to((-0.5 * width, top, 0), (0.55 * width, top, 0), (width, y, 0))
            .add_cubic_bezier_curve_to((0.75 * width, low, 0), (-0.60 * width, low, 0), (-width, y, 0))
            .close_path())


def character(body, x_offset, colors, pose):
    light = colors[0]
    bend = pose.joint("upper")[2]
    amount = min(1, abs(bend))
    y_offset = FLOOR - body.get_bottom()[1]
    parts = [body, stroke([(-0.62, 0.36), (-0.59, 0.52), (-0.49, 0.62), (-0.36, 0.65)], light, 4.4),
             mouth(amount)]
    for side in (-1, 1):
        ex, ey = 0.235 * side, 0.11
        openness = 0.94 + 0.20 * amount
        brow_y = 0.30 + 0.11 * openness
        parts.append(stroke([(ex - 0.12, brow_y), (ex - 0.06, brow_y + 0.07),
                             (ex + 0.07, brow_y + 0.073), (ex + 0.11, brow_y + side * 0.016)],
                            INK, 3.4))
        parts.append(oval(0.34, 0.32 * openness, ex, ey, CREAM, INK))
        px, py = ex + 0.042 * amount, ey + 0.058 * amount
        parts.append(oval(0.155, 0.157 * openness, px, py, INK))
        parts.append(oval(0.047, 0.046 * openness, px - 0.023, py + 0.026, CREAM))
        parts.append(oval(0.12, 0.079, ex + side * 0.18, -0.095, light))

    # Retarget the shell and face together for the ordinary Transform.
    artwork = VGroup(parts[0], VGroup(*parts[1:]))
    return pose.target_for(artwork).shift((x_offset, y_offset, 0))


class ShapeCharacters(Scene):
    def _bind_outline(self, body):
        bottom = body.get_bottom()[1]
        height = body.get_top()[1] - bottom
        return self.retarget_rig_2d(
            joints=[
                Joint2D("base", offset=(0, bottom + 0.21 * height)),
                Joint2D("lower", parent="base", offset=(0, 0.37 * height)),
                Joint2D("upper", parent="lower", offset=(0, 0.36 * height)),
            ],
            tracks=SOURCE_CLIP,
            mapping={"shoulder": ("lower", -0.44), "wrist": ("upper", -0.15)},
        )

    def construct(self):
        paper = NoisePaint(color_from_hex("#FCF3DF"), color_from_hex("#F1E7D0"),
                           seed=48, frequency=(135, 170))
        self.add(Rectangle(width=14.4, height=8.3, stroke=None).set_fill(paper, opacity=1))
        self.add(Text("One bend, three outlines", color=color_from_hex(INK),
                      font_size=39).move_to((0, 2.95, 0)))
        cast = [
            (Square(side_length=2.05), -4.49, ("#FBB295", "#E88971", "#9E4849")),
            (Circle(radius=1.06), -0.24, ("#A5DDB6", "#64AF94", "#2E685F")),
            (Star(n=5, outer_radius=1.35, inner_radius=0.80), 3.92,
             ("#FFE39B", "#F1B952", "#AB682D")),
        ]
        for body, x, (_, main, deep) in cast:
            body.insert_n_curves(28).set_fill(main, opacity=1).set_stroke(deep, width=2.2)
            self.add(oval(2.35, 0.13, x, FLOOR - 0.07, "#A38662").set_fill(opacity=0.24))

        rigs = [self._bind_outline(body) for body, _, _ in cast]
        def poses_at(time):
            return [character(*appearance, rig.pose(time)) for appearance, rig in zip(cast, rigs)]

        characters = poses_at(0)
        self.add(*characters)

        def transforms_to(time):
            return [Transform(source, target) for source, target in zip(characters, poses_at(time))]
        previous_time = 0.96
        self.wait(previous_time)
        for time in POSE_TIMES:
            duration = time - previous_time
            if (previous_time, time) in ((2.70, 3.15), (3.96, 4.32)):
                self.wait(duration)
            else:
                self.play(*transforms_to(time), run_time=duration, rate_func=smooth)
            previous_time = time
        self.play(*transforms_to(REST_TIME), run_time=REST_TIME - previous_time, rate_func=smooth)
        self.wait(7.2 - REST_TIME)
