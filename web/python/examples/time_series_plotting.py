"""Numeric labels and time-synchronized playback of illustrative measurements.

Paired with noon::time_series_plotting_example on native and direct WASM.
This is not an INS/GNSS simulation. The full dim line is a reference; bright
segments, marker, and cursor advance together according to data timestamps.
"""
from noon import *

SAMPLES = ((0.0, 0.4), (0.5, 1.0), (1.5, 1.7), (2.0, 1.2),
           (4.0, 0.6), (7.0, 2.1), (10.0, 1.4))
RUN_TIME = 6.0


class TimeSeriesPlotting(Scene):
    def construct(self):
        axes = Axes((0, 10, 2), (0, 2.5, 0.5), x_length=10, y_length=4)
        self.add(axes)
        axes.add_coordinates(
            x_config={"decimal_places": 0, "exclude_zero": False},
            y_config={"decimal_places": 1, "exclude_zero": True},
        )
        for text, size, y in (
            ("Time-synchronized sampled data", 28, 3.1),
            ("Uneven timestamps; one shared clock", 18, 2.55),
            ("Data time (s)", 20, -2.85),
        ):
            self.add(Text(text, font_size=size).move_to((0, y)))

        plan = axes.time_series_plan(SAMPLES, run_time=RUN_TIME)
        self.add(axes.plot_samples(SAMPLES, color=WHITE, stroke_width=1.8, opacity=0.25))
        cursor = Line(axes.c2p(SAMPLES[0][0], 0), axes.c2p(SAMPLES[0][0], 2.5),
                      color=GREEN, stroke_width=2.5)
        marker = Circle(radius=0.075, color=YELLOW, fill_opacity=1, stroke_width=0)
        marker.move_to(plan.points[0])
        self.add(cursor, marker)
        segments = [Line(start, end, color=BLUE, stroke_width=4)
                    for start, end in zip(plan.points, plan.points[1:])]
        for index, duration in enumerate(plan.durations):
            self.play(
                Create(segments[index]),
                marker.animate.move_to(plan.points[index + 1]),
                cursor.animate.move_to(plan.cursor_points[index + 1]),
                run_time=duration,
                rate_func=linear,
            )
        # Endpoint assertions use normal live getters after the final barrier.
        for actual, expected in ((marker.get_center(), plan.points[-1]),
                                 (cursor.get_center(), plan.cursor_points[-1])):
            assert abs(actual[0] - expected[0]) < 2e-5
            assert abs(actual[1] - expected[1]) < 2e-5
