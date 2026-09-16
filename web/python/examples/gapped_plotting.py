"""Explicit missing intervals: no bridge or stale marker during the blue outage.

Paired with noon::synchronized_plotting_example::gaps on native/direct WASM.
Measurements are illustrative, not actual GNSS or INS simulation output.
"""
from noon import *

RECORDINGS = (
    ((0, 0.4), (0.5, 1.0), (2, 0.8), (5, 1.2), (10, 0.6)),
    ((-1, 2.5), (1.5, 1.9), (4, 2.4), (8, 1.8), (12, 2.6)),
)
RUN_TIME = 6.0


class GappedPlotting(Scene):
    def construct(self):
        axes = Axes((0, 10, 2), (0, 3, 1), x_length=10, y_length=4)
        plan = axes.gapped_series_plan(
            RECORDINGS, break_after=((2,), ()), time_range=(0, 10), run_time=RUN_TIME,
        )
        self.add(axes)
        for axis, direction, exclude_zero in ((axes.x_axis, DOWN, False), (axes.y_axis, LEFT, True)):
            for label in axis.label_plan(exclude_zero=exclude_zero):
                self.add(Text(label.text, font_size=18).next_to(label.point, direction, buff=0.12))
        for source, size, x, y, tint in (
            ("Missing measurements stay missing", 28, 0, 3.2, WHITE),
            ("Explicit blue gap: 2-5 s; orange recording continues", 17, 0, 2.72, WHITE),
            ("Data time (s)", 20, 0, -2.85, WHITE),
            ("Interrupted recording", 18, -2.4, 2.25, BLUE),
            ("Continuous recording", 18, 2.4, 2.25, ORANGE),
        ):
            self.add(Text(source, font_size=size, color=tint).move_to((x, y)))
        for row in plan.series_segments:
            for segment in row:
                if segment is not None:
                    self.add(Line(*segment, color=WHITE, stroke_width=1.8).set_opacity(0.2))
        cursor = Line(axes.c2p(0, 0), axes.c2p(0, 3), color=GREEN, stroke_width=2.5)
        self.add(cursor)
        markers, segments = [], []
        for points, row, tint in zip(plan.series_points, plan.series_segments, (BLUE, ORANGE)):
            marker = Circle(radius=0.08, color=tint, fill_opacity=1, stroke_width=0).move_to(points[0])
            self.add(marker)
            markers.append(marker)
            segments.append(tuple(None if pair is None else Line(*pair, color=tint, stroke_width=4)
                                  for pair in row))
        shown = [True] * len(markers)
        for interval, duration in enumerate(plan.durations):
            animations = []
            for row, marker in enumerate(markers):
                pair = plan.series_segments[row][interval]
                if pair is None:
                    if shown[row]:
                        self.remove(marker)
                        shown[row] = False
                    continue
                if not shown[row]:
                    marker.move_to(pair[0])
                    self.add(marker)
                    shown[row] = True
                animations.extend((Create(segments[row][interval]), marker.animate.move_to(pair[1])))
            animations.append(cursor.animate.move_to(plan.cursor_points[interval + 1]))
            self.play(*animations, run_time=duration, rate_func=linear)
        for marker, points in zip(markers, plan.series_points):
            actual, expected = marker.get_center(), points[-1]
            assert abs(actual[0] - expected[0]) < 2e-5
            assert abs(actual[1] - expected[1]) < 2e-5
