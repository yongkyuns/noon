"""Paired Python proof for canonical predeclared affine live animation.

Run in the Noon browser/Pyodide authoring environment. The target and animation
are declared through shared Rust semantic handles before the live session starts;
Python only invokes play/advance/wait on that one Rust runtime.
"""

from noon import Circle, Scene, linear


class LiveAffineAnimation(Scene):
    def construct(self):
        circle = Circle(1.0)
        self.add(circle)

        target = circle.copy().shift((4.0, -2.0, 0.0)).scale(2.0)
        animation = self.declare_live_transform_to(
            circle,
            target,
            run_time=2.0,
            rate_func=linear,
        )

        live = self.live_execution()
        end_time = live.play(animation)
        assert not live.advance_to(1.0)
        assert live.effective_center(circle) == (2.0, -1.0)
        assert not live.advance_to(end_time)
        live.complete()
        assert live.effective_center(circle) == (4.0, -2.0)

        wait_end = live.wait(0.25)
        assert live.advance_to(wait_end)
        live.complete()
