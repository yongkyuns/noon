"""Paired Python proof for shared Rust callback paint semantics.

Rust owns preservation of enabled fill/stroke alpha and opacity-only fill
behavior. Python supplies color coercion and the arbitrary callable bodies.
"""

from noon import Circle, Color, Scene, linear


class LiveCallbackPaint(Scene):
    def construct(self):
        circle = (
            Circle(1.0)
            .set_fill(Color(0.1, 0.2, 0.8), opacity=0.25)
            .set_stroke(Color(0.9, 0.9, 0.9), width=0.12, opacity=0.75)
        )
        self.add(circle)
        target = circle.copy().shift((2.0, 0.0, 0.0))
        animation = self.declare_live_transform_to(
            circle,
            target,
            run_time=1.0,
            rate_func=linear,
        )

        def recolor(mobject, _dt):
            mobject.set_color(Color(0.8, 0.4, 0.2, 0.9))

        def fill_and_composite_opacity(mobject, _dt):
            mobject.set_fill(opacity=0.4)
            # Callback set_opacity remains the separately qualified object
            # composite domain rather than Manim's ordinary paint-alpha edit.
            mobject.set_opacity(0.5)

        circle.add_updater(recolor)
        circle.add_updater(fill_and_composite_opacity)
        live = self.live_execution()
        assert live.play(animation) == 1.0
