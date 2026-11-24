from noon import *
import json

def close(actual, expected, tolerance=2e-5):
    assert len(actual) == len(expected), (actual, expected)
    assert all(abs(a - b) <= tolerance for a, b in zip(actual, expected)), (actual, expected)

def report(case, observations):
    print("NOON_HOST_REPORT " + json.dumps({"case": case, "observations": observations}, allow_nan=False))

import asyncio

class Cancellation(Scene):
    def setup(self):
        self.observations = []

    async def construct(self):
        self.marker = Circle(0.5)
        self.add(self.marker)
        try:
            # Cancellation is requested after eager admission, before the host
            # consumes its next completion. No backend-specific source branch.
            pending = self.play(self.marker.animate.shift(RIGHT), run_time=1, rate_func=linear)
            self.observations.append(["admitted", self.time])
            asyncio.current_task().cancel()
            await pending
            self.observations.append(["incorrectly-resumed"])
        finally:
            self.observations.append(["finally"])

    def tear_down(self):
        self.observations.append(["tear-down"])
        assert [v[0] for v in self.observations] == ["admitted", "finally", "tear-down"]
        report("cancellation", self.observations)
