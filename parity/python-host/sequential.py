from noon import *
import json

def close(actual, expected, tolerance=2e-5):
    assert len(actual) == len(expected), (actual, expected)
    assert all(abs(a - b) <= tolerance for a, b in zip(actual, expected)), (actual, expected)

def report(case, observations):
    print("NOON_HOST_REPORT " + json.dumps({"case": case, "observations": observations}, allow_nan=False))

class Sequential(Scene):
    async def reveal(self, target):
        await self.play(Create(target), run_time=0.25, rate_func=linear)

    async def construct(self):
        a = Circle(0.5)
        await self.reveal(a)
        checkpoints = [[self.time, *a.get_center()]]
        for direction in (RIGHT, UP):
            try:
                await self.play(a.animate.shift(direction), run_time=0.25, rate_func=linear)
            finally:
                checkpoints.append([self.time, *a.get_center()])
        close(a.get_center(), (1, 1))
        a.move_to((2, -1, 0))
        close(a.get_center(), (2, -1))
        b = Square(0.5).move_to(a.get_center())
        self.add(b)
        await self.wait(0.25)
        close(b.get_center(), (2, -1))
        assert self.time == 1.0
        report("sequential", checkpoints + [[self.time, *b.get_center()]])
