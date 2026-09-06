"""Canonical callback fixture for target-local renderer observation.

The two callbacks affect disjoint objects. The harness changes only their authored
registration order so Rust selects either the geometry or Text target first while
committing the same coherent callback phase and rendering the same final frame.
"""

from noon import Circle, Scene, Text, WHITE, linear


class RendererObservationCallbacks(Scene):
    def construct(self):
        animated = Circle(0.8).set_fill(WHITE, opacity=1.0)
        label = Text("Noon", font_size=48).shift((0.0, -2.0, 0.0))
        anchor = Circle(0.4).set_fill(WHITE, opacity=1.0).shift((-3.0, 0.0, 0.0))
        self.add(animated, label, anchor)

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

        def move_label(mobject, _dt):
            mobject.move_to((1.0, -2.0, 0.0))

        if context.get("observation_target") == "geometry":
            animated.add_updater(lift)
            label.add_updater(move_label)
        else:
            label.add_updater(move_label)
            animated.add_updater(lift)

        live = self.live_execution()
        assert live.play(animation) == 2.0


result = RendererObservationCallbacks()
