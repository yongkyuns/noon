"""Paired Python proof for canonical required affine property callbacks.

The animation declaration and all callback occurrences are authored before the
one Rust execution session exists. During playback Rust selects the occurrences,
prepares only their active object rows, and Python returns one ordered effective
property batch through the Pyodide worker boundary. This source deliberately does
not seek or replay opaque callbacks: browser execution advances it forward.
"""

from noon import Circle, Scene, Text, WHITE, linear


class LiveAffineCallbacks(Scene):
    def construct(self):
        label = Text("Noon", font_size=48).shift((0.0, -2.0, 0.0))
        animated = Circle(1.0).set_fill(WHITE, opacity=1.0)
        drift = Circle(0.5).set_fill(WHITE, opacity=1.0).shift((-3.0, 0.0, 0.0))
        self.add(label, animated, drift)

        target = animated.copy().shift((2.0, 0.0, 0.0))
        animation = self.declare_live_transform_to(
            animated,
            target,
            run_time=2.0,
            rate_func=linear,
        )

        def lift(mobject, _dt):
            center = mobject.get_center()
            mobject.move_to((center.x, 1.0, 0.0))

        def style_after_lift(mobject, _dt):
            assert mobject.get_center().y == 1.0
            mobject.set_opacity(0.5)

        def accumulate_drift(mobject, dt):
            mobject.shift((0.0, dt, 0.0))

        def accumulate_text(mobject, dt):
            mobject.shift((dt, 0.0, 0.0))

        label.add_updater(accumulate_text)
        animated.add_updater(lift)
        animated.add_updater(style_after_lift)
        drift.add_updater(accumulate_drift)

        # This leases the same session later used by the execution worker. It
        # does not execute callbacks during authoring: their required phase is
        # completed only by forward worker playback.
        live = self.live_execution()
        assert live.play(animation) == 2.0
