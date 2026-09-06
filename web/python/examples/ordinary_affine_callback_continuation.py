"""Async required callbacks over one canonical affine continuation session.

The source remains suspended while Rust advances the segment. Each Rust-issued
phase invokes these Python callables on that same suspended stack, commits one
token-pinned effective batch, and returns to Rust. ``construct`` resumes only
when the segment has completed and its player lease was returned.
"""

import _manim_updaters
from noon import Circle, Color, Scene, Transform, linear


class OrdinaryAffineCallbackContinuation(Scene):
    async def construct(self):
        circle = Circle(radius=0.4).set_fill(Color(0.0, 0.4, 1.0), opacity=1.0)
        phase_counts: dict[float, int] = {}

        def lift(mobject, _dt):
            phase_time = _manim_updaters._canonical_callback_time(mobject)
            phase_counts[phase_time] = phase_counts.get(phase_time, 0) + 1
            assert phase_counts[phase_time] == 1
            center = mobject.get_center()
            mobject.move_to((center.x, 1.0, 0.0))

        def dim_after_lift(mobject, _dt):
            assert mobject.get_center().y == 1.0
            mobject.set_opacity(0.5)

        circle.add_updater(lift)
        circle.add_updater(dim_after_lift)
        self.add(circle)

        target = circle.copy().shift((2.0, 0.0, 0.0))
        await self.play(Transform(circle, target), run_time=1.0, rate_func=linear)

        assert phase_counts.get(0.0) == 1
        assert phase_counts.get(1.0) == 1
        assert len(phase_counts) >= 2
        assert self.time == 1.0
        assert circle.get_center() == (2.0, 1.0)
