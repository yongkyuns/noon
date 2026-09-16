"""Coordinates constructed after a logical wait, using the existing runtime.

Paired with noon::live_coordinate_plotting_example on native and direct WASM.
Numeric-label creation remains cold-only; add late axes before querying them.
"""
from noon import *


class LiveCoordinatePlotting(Scene):
    def construct(self):
        title = Text("Create axes after playback starts", font_size=26).move_to((0, 3))
        sentinel = Circle(radius=0.12, color=GREEN, fill_opacity=1, stroke_width=0).move_to((-5, 2.6))
        self.add(title, sentinel)
        self.wait(0.25)
        axes = Axes((-2, 2, 1), (-1, 1, 1), x_length=8, y_length=3).shift((0, -0.5))
        line = NumberLine((0, 4, 1), length=4, color=ORANGE).shift((0, 2))
        self.add(axes, line)
        assert abs(line.p2n(line.n2p(2)) - 2) < 2e-5
        self.play(axes.animate.shift((0.5, 0)), run_time=0.5, rate_func=linear)
        origin = axes.c2p(0, 0)
        assert abs(origin.x - 0.5) < 2e-5 and abs(origin.y + 0.5) < 2e-5
        graph = axes.plot(lambda x: 0.5 * x, (-2, 2, 0.5), use_smoothing=False, color=BLUE, stroke_width=4)
        self.play(Create(graph), run_time=0.5, rate_func=linear)
        self.wait(0.25)
        assert abs(sentinel.get_center().x + 5) < 2e-5
