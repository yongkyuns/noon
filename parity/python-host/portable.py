from noon import *
import json

def close(actual, expected, tolerance=2e-5):
    assert len(actual) == len(expected), (actual, expected)
    assert all(abs(a - b) <= tolerance for a, b in zip(actual, expected)), (actual, expected)

def report(case, observations):
    print("NOON_HOST_REPORT " + json.dumps({"case": case, "observations": observations}, allow_nan=False))

class Portable(Scene):
    def construct(self):
        a = Circle(0.5)
        self.add(a)
        self.play(a.animate.shift(RIGHT), run_time=0.25, rate_func=linear)
        self.play(a.animate.shift(UP), run_time=0.25, rate_func=linear)
        self.wait(0.5)
        close(a.get_center(), (1, 1))
        report("portable", [[self.time, *a.get_center()]])
