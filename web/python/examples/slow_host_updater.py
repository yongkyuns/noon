"""Required slow callbacks preserve updater order and authored time in Rust."""

import time
from noon import BLUE, Circle, LEFT, RED, RIGHT, Scene, Square, linear


class SlowHostUpdaterScene(Scene):
    async def construct(self):
        dot = Circle(radius=0.45, color=BLUE).shift(LEFT * 2)
        native = Square(side_length=0.7, color=RED).shift(LEFT)
        elapsed = 0.0
        ordered_calls = 0
        slow_calls = 0

        def slow_update(mobject, dt):
            nonlocal elapsed, slow_calls
            # Deliberately exceed a 60 Hz deadline. This required phase must
            # complete before Rust publishes later authored state.
            deadline = time.perf_counter() + 0.080
            while time.perf_counter() < deadline:
                pass
            elapsed += dt
            slow_calls += 1
            mobject.shift(RIGHT * dt)

        def observe_update(mobject, _dt):
            nonlocal ordered_calls
            ordered_calls += 1
            assert ordered_calls == slow_calls
            assert abs(mobject.get_center().x - (-2.0 + elapsed)) < 1e-6

        dot.add_updater(slow_update)
        dot.add_updater(observe_update)
        self.add(dot, native)
        await self.play(native.animate.shift(RIGHT * 3), run_time=0.4, rate_func=linear)
        assert slow_calls == ordered_calls and slow_calls >= 2
        assert abs(elapsed - 0.4) < 1e-6
        assert abs(self.time - 0.4) < 1e-6
        assert abs(dot.get_center().x + 1.6) < 1e-6
        assert abs(native.get_center().x - 2.0) < 1e-6
