from noon import *
import json

def close(actual, expected, tolerance=2e-5):
    assert len(actual) == len(expected), (actual, expected)
    assert all(abs(a - b) <= tolerance for a, b in zip(actual, expected)), (actual, expected)

def report(case, observations):
    print("NOON_HOST_REPORT " + json.dumps({"case": case, "observations": observations}, allow_nan=False))

class Atomicity(Scene):
    async def construct(self):
        a = Circle(0.5)
        b = Square(0.5).shift(RIGHT)
        self.add(a, b)
        await self.wait(0.25)
        observations = []
        baseline = (tuple(a.get_center()), tuple(b.get_center()), self.time, list(self.mobjects))
        try:
            # Shared Rust admission rejects two simultaneous writers to one channel.
            await self.play(a.animate.shift(RIGHT), a.animate.shift(UP), run_time=0.25, rate_func=linear)
        except Exception as error:
            observations.append(["conflict", type(error).__name__, getattr(error, "category", None)])
        else:
            raise AssertionError("conflicting composition was admitted")
        assert baseline == (tuple(a.get_center()), tuple(b.get_center()), self.time, list(self.mobjects))
        try:
            a.set_fill(BLUE, opacity=2)
        except ValueError as error:
            observations.append(["invalid-opacity", type(error).__name__])
        else:
            raise AssertionError("invalid opacity was accepted")
        assert baseline == (tuple(a.get_center()), tuple(b.get_center()), self.time, list(self.mobjects))
        await self.play(a.animate.shift(UP), run_time=0.25, rate_func=linear)
        await self.wait(0.5)
        close(a.get_center(), (0, 1))
        report("atomicity", observations + [["recovered", self.time, *a.get_center(), *b.get_center()]])
