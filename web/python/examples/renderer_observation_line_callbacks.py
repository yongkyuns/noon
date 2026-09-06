"""Canonical Circle/Line/Text fixture for local Line callback observation.

The moving Line is the only callback target. Its forward and reverse windows are
authored before the one Rust execution session exists; the Circle, reference
Line, and native Text remain resident siblings throughout the exact-time probes.
"""

from noon import BLUE, LEFT, ORIGIN, WHITE, YELLOW, Circle, Line, Scene, Text


class RendererObservationLineCallbacks(Scene):
    def construct(self):
        marker = Circle(0.35).set_color(BLUE).shift((-3.0, 0.0, 0.0))
        reference = Line(ORIGIN, LEFT).set_color(WHITE)
        moving = Line(ORIGIN, LEFT).set_color(YELLOW)
        label = Text("Noon", font_size=48).shift((0.0, -2.0, 0.0))

        def forward(mobject, dt):
            mobject.rotate_about_origin(dt)

        def backward(mobject, dt):
            mobject.rotate_about_origin(-dt)

        moving.add_updater(forward)
        self.add(marker, reference, moving, label)
        self.wait(2.0)
        moving.remove_updater(forward)
        moving.add_updater(backward)
        self.wait(2.0)
        moving.remove_updater(backward)
        self.wait(0.5)

        # The waits above author callback activation windows only. This creates
        # the one execution session that the worker advances monotonically.
        self.live_execution(duration=4.5)
