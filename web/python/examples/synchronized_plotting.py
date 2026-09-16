"""Two illustrative recordings on one data clock, paired with typed Rust.

Dim complete curves are references. Bright segments and markers follow shared
intervals from Rust, even though the recordings have different sample times.
This small example is not an INS/GNSS simulation or a streaming player.
"""
from noon import *

RECORDINGS = (
    ((0, 0.4), (0.5, 1.0), (2, 0.8), (5, 1.2), (10, 0.6)),
    ((-1, 2.5), (1.5, 1.9), (4, 2.4), (8, 1.8), (12, 2.6)),
)
RUN_TIME = 6.0


class SynchronizedPlotting(Scene):
    def construct(self):
        axes = Axes((0, 10, 2), (0, 3, 1), x_length=10, y_length=4)
        plan = axes.synchronized_series_plan(RECORDINGS, time_range=(0, 10), run_time=RUN_TIME)
        self.add(axes)
        for axis, direction, exclude_zero in ((axes.x_axis, DOWN, False), (axes.y_axis, LEFT, True)):
            for label in axis.label_plan(decimal_places=0, exclude_zero=exclude_zero):
                self.add(Text(label.text, font_size=18).next_to(label.point, direction, buff=0.12))
        for text, size, x, y, color in (
            ("Two recordings, one data clock", 28, 0, 3.2, WHITE),
            ("Different sample times; piecewise-linear interpolation", 17, 0, 2.72, WHITE),
            ("Data time (s)", 20, 0, -2.85, WHITE),
            ("Series A", 18, -2, 2.25, BLUE),
            ("Series B", 18, 2, 2.25, ORANGE),
        ):
            self.add(Text(text, font_size=size, color=color).move_to((x, y)))
        # Plan points are already in world coordinates. Do not map them again.
        for points in plan.series_points:
            reference = VMobject(color=WHITE, stroke_width=1.8, opacity=0.2)
            reference.set_points_as_corners(points)
            self.add(reference)
        cursor = Line(axes.c2p(0, 0), axes.c2p(0, 3), color=GREEN, stroke_width=2.5)
        self.add(cursor)
        markers, segments = [], []
        for points, color in zip(plan.series_points, (BLUE, ORANGE), strict=True):
            marker = Circle(radius=0.08, color=color, fill_opacity=1, stroke_width=0).move_to(points[0])
            self.add(marker)
            markers.append(marker)
            segments.append([Line(a, b, color=color, stroke_width=4) for a, b in zip(points, points[1:])])
        for i, duration in enumerate(plan.durations):
            animations = []
            for points, marker, row in zip(plan.series_points, markers, segments, strict=True):
                animations.extend((Create(row[i]), marker.animate.move_to(points[i + 1])))
            animations.append(cursor.animate.move_to(plan.cursor_points[i + 1]))
            self.play(*animations, run_time=duration, rate_func=linear)
        for marker, points in zip(markers, plan.series_points, strict=True):
            assert abs(marker.get_center()[0] - points[-1][0]) < 2e-5
            assert abs(marker.get_center()[1] - points[-1][1]) < 2e-5
