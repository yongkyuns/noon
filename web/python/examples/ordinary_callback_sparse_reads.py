"""Sparse callback reads over one suspended canonical continuation.

Rust supplies the callback target eagerly. The updater lazily reads a scalar
ValueTracker and a static anchor's effective bounds through the exact pending
callback phase, then resumes this same Python invocation. No callback is
restarted to fill either row.
"""

import _manim_updaters
from noon import Circle, Color, Scene, linear


class OrdinaryCallbackSparseReads(Scene):
    async def construct(self):
        anchor = (
            Circle(radius=0.4)
            .set_fill(Color(0.0, 0.4, 1.0), opacity=1.0)
            .shift((-1.0, 1.0, 0.0))
        )
        circle = Circle(radius=0.4).set_fill(Color(0.0, 0.4, 1.0), opacity=1.0)
        tracker = self.value_tracker(0.0)
        phase_counts: dict[float, int] = {}

        def follow_sparse_values(mobject, _dt):
            phase_time = _manim_updaters._canonical_callback_time(mobject)
            phase_counts[phase_time] = phase_counts.get(phase_time, 0) + 1
            assert phase_counts[phase_time] == 1
            anchor_center = anchor.get_center()
            mobject.move_to((anchor_center.x + tracker.get_value(), anchor_center.y, 0.0))

        self.add(anchor, circle)
        circle.add_updater(follow_sparse_values)

        # The first wait reaches an initial callback phase before any scalar
        # track exists. This proves callback reads do not depend on active or
        # touched signal rows.
        await self.wait(0.25)
        assert circle.get_center() == (-1.0, 1.0)
        await self.play(tracker.animate.set_value(2.0), run_time=1.0, rate_func=linear)

        # The timed track has completed. Rust appends the persistent hold, and
        # the following wait proves the scalar remains callback-readable while
        # it has no active property binding.
        tracker.set_value(3.0)
        await self.wait(0.25)

        assert phase_counts.get(0.0) == 1
        assert self.time == 1.5
        assert circle.get_center() == (2.0, 1.0)
