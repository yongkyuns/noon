from noon import *
import time


class SlowHostUpdaterScene(Scene):
    def construct(self):
        dot = Circle(radius=0.45, color=BLUE).shift(LEFT * 2)
        native = Square(side_length=0.7, color=RED).shift(LEFT)

        def slow_update(mobject, dt):
            # Intentionally exceed a 60 Hz frame deadline. This runs in the
            # Python worker; the engine/render workers must remain responsive.
            deadline = time.perf_counter() + 0.080
            while time.perf_counter() < deadline:
                pass
            mobject.shift(RIGHT * 0.02)

        dot.add_updater(slow_update)
        self.add(dot, native)
        self.play(native.animate.shift(RIGHT * 3), run_time=2.0)
