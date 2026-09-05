"""Canonical scalar ValueTracker proof over one shared Rust session.

The tracker, derived position, deterministic track, effective reads, and rendered
handoff all stay in the canonical SemanticStore -> ExecutionSession path.
"""

from noon import Circle, RIGHT, Scene, WHITE, linear


class LiveValueTracker(Scene):
    def construct(self):
        circle = Circle(radius=0.4, color=WHITE, fill_opacity=1.0)
        self.add(circle)

        progress = self.value_tracker(0.0)
        self.bind_position(circle, progress, direction=RIGHT, offset=(-2.0, 0.0, 0.0))
        self.play(progress.animate(run_time=2.0, rate_func=linear).set_value(4.0))
        assert self.time == 2.0
        assert progress.get_value() == 4.0

        live = self.live_execution(duration=2.0)
        live.evaluate(1.0)
        assert progress.get_value() == 2.0
        assert live.effective_center(circle) == (0.0, 0.0)

        live.evaluate(2.0)
        assert progress.get_value() == 4.0
        assert live.effective_center(circle) == (2.0, 0.0)

        try:
            progress.set_value(3.0)
            raise AssertionError("timeline-owned tracker accepted a direct input write")
        except ValueError as error:
            assert "timeline" in str(error).lower()
