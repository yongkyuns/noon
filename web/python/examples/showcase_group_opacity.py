"""Slide in overlapping tutorial highlights as one fading layer."""
from noon import *


class OverlayFade(Scene):
    def construct(self):
        def spot(x):
            return Circle(radius=0.82, fill=BLUE, fill_opacity=1, stroke=None).shift(
                x * RIGHT + 0.2 * DOWN)

        combined = VGroup(spot(-2.7), spot(-1.8)).shift(DOWN).set_object_opacity(0)
        first, second = spot(1.8), spot(2.7)
        first.set_object_opacity(0)
        second.set_object_opacity(0)
        self.add(
            Text("As a group", font_size=34).shift(2.25 * LEFT + 1.27 * UP),
            Text("Per shape", font_size=34).shift(2.25 * RIGHT + 1.27 * UP),
            first, second,
        )
        self.wait(0.6)
        self.play(combined.animate.shift(UP).set_object_opacity(0.42),
                  first.animate.set_object_opacity(0.42),
                  second.animate.set_object_opacity(0.42),
                  run_time=1.2, rate_func=linear)
        self.wait(1.2)
        self.play(combined.animate.set_object_opacity(1),
                  first.animate.set_object_opacity(1),
                  second.animate.set_object_opacity(1),
                  run_time=1, rate_func=linear)
        self.wait(0.8)
