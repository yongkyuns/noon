"""Async required callbacks over one canonical affine continuation session.

The source remains suspended while Rust advances the segment. Each Rust-issued
phase invokes these Python callables on that same suspended stack, commits one
token-pinned effective batch, and returns to Rust. ``construct`` resumes only
when the segment has completed and its player lease was returned.
"""

import _manim_updaters
from noon import Circle, Color, RIGHT, Scene, Square, Transform, VGroup, linear


class OrdinaryAffineCallbackContinuation(Scene):
    async def construct(self):
        circle = Circle(radius=0.4).set_fill(Color(0.0, 0.4, 1.0), opacity=1.0)
        self.add(circle)
        family = VGroup(circle)
        probe = Square(0.2)  # Detached before a live context is attached to wrappers.
        phase_counts: dict[float, int] = {}

        def lift(mobject, _dt):
            phase_time = _manim_updaters._canonical_callback_time(mobject)
            phase_counts[phase_time] = phase_counts.get(phase_time, 0) + 1
            assert phase_counts[phase_time] == 1
            if phase_time == 0.0:
                try:
                    family.get_center()
                except NotImplementedError as error:
                    assert "active callback phase" in str(error)
                else:
                    raise AssertionError("family query bypassed the active callback overlay")
            center = mobject.get_center()
            mobject.move_to((center.x, 1.0, 0.0))

        def dim_after_lift(mobject, _dt):
            assert mobject.get_center().y == 1.0
            mobject.set_fill(opacity=0.75)
            mobject.set_opacity(0.5)

        circle.add_updater(lift)
        circle.add_updater(dim_after_lift)
        # This copy intentionally happens after callbacks are registered. The
        # narrow canonical target-editor route must retain the opaque semantic
        # handle without copying the callback registry or raw geometry.
        target = circle.copy().shift((2.0, 0.0, 0.0))
        await self.play(Transform(circle, target), run_time=1.0, rate_func=linear)

        assert phase_counts.get(0.0) == 1
        assert phase_counts.get(1.0) == 1
        assert len(phase_counts) >= 2
        assert self.time == 1.0
        assert circle.get_center() == (2.0, 1.0)
        # Registered callbacks keep their Rust-owned effective values after the
        # phase completes; family queries must not materialize raw geometry.
        def reject_raw_projection():
            raise AssertionError("callback family query materialized Python geometry")
        circle._current_raw = reject_raw_projection
        # Object-composite dimming must not alter observed fill/stroke alpha.
        assert circle.get_fill_opacity() == 0.75
        assert circle.get_stroke_opacity() == 1.0
        copied = circle.copy()
        copied._current_raw = reject_raw_projection
        copied.set_color(Color(1.0, 0.0, 0.0))
        assert copied.get_fill_opacity() == 0.75
        copied.set_fill(opacity=0.5)
        assert copied.get_fill_opacity() == 0.5
        copied.set_stroke(opacity=0.25)
        assert copied.get_stroke_opacity() == 0.25
        copied.set_opacity(0.4)
        assert abs(copied.get_fill_opacity() - 0.4) < 1e-6
        assert abs(copied.get_stroke_opacity() - 0.4) < 1e-6
        assert family.get_center() == (2.0, 1.0)
        assert abs(family.width - 0.8) < 1e-6
        assert abs(family.height - 0.8) < 1e-6
        # A plain live placement can reference the callback's effective bounds.
        probe.next_to(circle, RIGHT, buff=0.1)
        assert abs(probe.get_center().x - 2.6) < 1e-6
        assert abs(probe.get_center().y - 1.0) < 1e-6
        before = circle.get_center()
        try:
            circle.next_to(probe, RIGHT, buff=0.1)
        except ValueError as error:
            assert "active effective affine driver" in str(error)
        else:
            raise AssertionError("placement bypassed the active affine driver")
        assert circle.get_center() == before
