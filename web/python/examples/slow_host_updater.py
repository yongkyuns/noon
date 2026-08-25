from noon import *
import time


class SlowHostUpdaterScene(Scene):
    def construct(self):
        dot = Circle(radius=0.45, color=BLUE)

        def slow_update(mobject, dt):
            # Intentionally exceed a 60 Hz frame deadline. This runs in the
            # Python worker; the engine/render workers must remain responsive.
            deadline = time.perf_counter() + 0.080
            while time.perf_counter() < deadline:
                pass
            mobject.shift(RIGHT * 0.02)

        dot.add_updater(slow_update)
        self.add(dot)
