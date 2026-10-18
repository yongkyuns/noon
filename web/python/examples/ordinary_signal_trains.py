"""Travelling signals using existing shared Rust animation composition.

Rust pair: noon::example_scenes::line_passing_flash::train_program().
The same Rust continuation can be recorded with train_playback() for direct seek.
The upper rail carries longer cyan pulses left-to-right; the lower rail carries
short yellow pulses right-to-left. The dim rails remain visible throughout.

Nine 1.6-second pulses staggered by 0.2 seconds make a finite 3.2-second train.
Changing time_width changes each pulse's visible fraction of the source line.
Changing the lag ratio changes spacing; reversing line endpoints changes flow
direction. This is not the general looping dotted/curved-stroke API in #2188.
There are no updaters or per-frame Python geometry calculations.
"""

from noon import *


class OrdinarySignalTrains(Scene):
    def construct(self):
        trains = []
        for start, end, color, time_width in [
            ([-3, 1, 0], [3, 1, 0], "#00ffff", 0.12),
            ([3, -1, 0], [-3, -1, 0], "#ffcc00", 0.025),
        ]:
            # Keep the connection separate from the transient signals. A
            # PassingFlash removes its own source when its animation completes.
            rail = Line(start, end, color="#404040", stroke_width=2.5)
            self.add(rail)
            signals = [
                ShowPassingFlash(
                    Line(start, end, color=color, stroke_width=10),
                    time_width=time_width,
                    run_time=1.6,
                    rate_func=linear,
                )
                for _ in range(9)
            ]
            trains.append(LaggedStart(*signals, lag_ratio=0.125, rate_func=linear))

        self.wait(0.5)
        self.play(*trains, run_time=3.2, rate_func=linear)
        self.wait(0.5)
