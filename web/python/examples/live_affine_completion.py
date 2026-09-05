"""Paired canonical continuation proof after affine completion.

The same Rust session owns both completion reconciliation and the next activated
segment. Python observes effective values and invokes shared operations only.
"""

from noon import Circle, Scene, linear


class LiveAffineCompletion(Scene):
    def construct(self):
        circle = Circle(1.0)
        self.add(circle)

        first_target = circle.copy().shift((2.0, -2.0, 0.0))
        second_target = circle.copy().shift((5.0, -2.0, 0.0))
        first = self.declare_live_transform_to(
            circle, first_target, run_time=2.0, rate_func=linear
        )
        second = self.declare_live_transform_to(
            circle, second_target, run_time=2.0, rate_func=linear
        )

        live = self.live_execution()
        first_end = live.play(first)
        assert not live.advance_to(first_end)
        live.complete()
        assert live.effective_center(circle) == (2.0, -2.0)

        live.set_translation(circle, 3.0, -2.0)
        wait_end = live.wait(0.25)
        assert live.advance_to(wait_end)
        live.complete()
        assert live.effective_center(circle) == (3.0, -2.0)

        second_end = live.play(second)
        assert not live.advance_to(second_end - 1.0)
        assert live.effective_center(circle) == (4.0, -2.0)
        assert circle.get_center() == (4.0, -2.0)
        assert circle.width == 2.0
        assert circle.height == 2.0
        assert not live.advance_to(second_end)
        live.complete()
        assert live.effective_center(circle) == (5.0, -2.0)
        assert circle.get_center() == (5.0, -2.0)
        assert circle.width == 2.0
        assert circle.height == 2.0
