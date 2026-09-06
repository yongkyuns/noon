"""Async scalar continuation over one shared Rust session.

The tracker timeline, persistent post-completion write, wait, and second
tracker segment all stay in Rust. Python only awaits the returned player lease.
"""

from noon import Circle, RIGHT, Scene, ValueTracker, WHITE, linear


# Constructed before the Scene instance and construct lifecycle. The worker's
# shared Rust store owns this value and identity while it is detached.
progress = ValueTracker(0.0)


class OrdinaryValueTrackerContinuation(Scene):
    async def construct(self):
        circle = Circle(radius=0.4, color=WHITE, fill_opacity=1.0)
        self.add(circle)

        self.bind_position(
            circle, progress, direction=RIGHT, offset=(-2.0, 0.0, 0.0)
        )

        await self.play(
            progress.animate(run_time=2.0, rate_func=linear).set_value(2.0)
        )
        assert self.time == 2.0
        assert progress.get_value() == 2.0
        assert circle.get_center() == (0.0, 0.0)

        progress.set_value(3.0)
        assert progress.get_value() == 3.0
        assert circle.get_center() == (1.0, 0.0)

        await self.wait(1.0)
        assert self.time == 3.0
        assert progress.get_value() == 3.0
        assert circle.get_center() == (1.0, 0.0)

        await self.play(
            progress.animate(run_time=1.0, rate_func=linear).set_value(5.0)
        )
        assert self.time == 4.0
        assert progress.get_value() == 5.0
        assert circle.get_center() == (3.0, 0.0)
