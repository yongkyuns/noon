"""Persistent diagram connectors with finite travelling messages.

This storyboard teaches one general diagram composition technique: durable
system blocks/links are ordinary Noon objects, while brief directional
messages use existing shared-Rust PassingFlash + LaggedStart semantics.
There is no Python frame updater, animation clock, path routing, or renderer.
This is a fixed straight-link demonstration, not continuous curved-flow M1.
"""
from noon import *


class DataLinks(Scene):
    def construct(self):
        title = Text("Signals travel. Connections remain.", font_size=33)
        title.move_to((0, 3.05, 0))
        caption = Text(
            "The same durable link carries different transient messages.",
            font_size=20, color=GRAY,
        ).move_to((0, 2.50, 0))

        left_box = Rectangle(
            width=2.35, height=2.2, color=BLUE, stroke_width=3,
        ).set_fill(BLUE, opacity=0.16)
        right_box = Rectangle(
            width=2.35, height=2.2, color=TEAL, stroke_width=3,
        ).set_fill(TEAL, opacity=0.16)
        source = VGroup(
            left_box,
            Text("SENSORS", font_size=26).move_to((0, 0.28, 0)),
            Text("IMU + GNSS", font_size=20, color=GRAY).move_to((0, -0.32, 0)),
        ).move_to((-4.55, 0, 0))
        fusion = VGroup(
            right_box,
            Text("FUSION", font_size=26).move_to((0, 0.28, 0)),
            Text("Estimator", font_size=20, color=GRAY).move_to((0, -0.32, 0)),
        ).move_to((4.55, 0, 0))

        outgoing = ((-3.15, 0.65, 0), (3.15, 0.65, 0))
        returning = ((3.15, -0.65, 0), (-3.15, -0.65, 0))
        data_rail = Line(*outgoing, color="#404040", stroke_width=2.5)
        status_rail = Line(*returning, color="#404040", stroke_width=2.5)
        data_label = Text(
            "SENSOR SAMPLES   →", font_size=19, color=TEAL,
        ).move_to((0, 1.23, 0))
        status_label = Text(
            "←   ESTIMATOR STATUS", font_size=19, color=YELLOW,
        ).move_to((0, -1.23, 0))
        resolved = Text(
            "Only the messages disappear — the connections stay.",
            font_size=20, color=GRAY,
        ).move_to((0, -2.55, 0))

        self.play(
            FadeIn(title), FadeIn(caption), FadeIn(source), FadeIn(fusion),
            run_time=0.9,
        )
        self.play(
            Create(data_rail), Create(status_rail),
            FadeIn(data_label), FadeIn(status_label),
            run_time=0.9,
        )
        self.wait(0.25)

        # Paired ordinary animations; all progression is in shared Rust.
        trains = []
        for start, end, color, width in [
            (outgoing[0], outgoing[1], "#00ffff", 0.12),
            (returning[0], returning[1], "#ffcc00", 0.025),
        ]:
            pulses = [
                ShowPassingFlash(
                    Line(start, end, color=color, stroke_width=10),
                    time_width=width,
                    run_time=1.6,
                    rate_func=linear,
                )
                for _ in range(9)
            ]
            trains.append(
                LaggedStart(*pulses, lag_ratio=0.125, rate_func=linear)
            )
        self.play(*trains, run_time=3.2, rate_func=linear)
        self.wait(0.5)
        self.play(FadeOut(caption), FadeIn(resolved), run_time=0.6)
        self.wait(0.85)
