from noon import *
import json

def close(actual, expected, tolerance=2e-5):
    assert len(actual) == len(expected), (actual, expected)
    assert all(abs(a - b) <= tolerance for a, b in zip(actual, expected)), (actual, expected)

def report(case, observations):
    print("NOON_HOST_REPORT " + json.dumps({"case": case, "observations": observations}, allow_nan=False))

from contextvars import ContextVar
scope = ContextVar("noon-host-conformance", default="unset")

class Callbacks(Scene):
    async def construct(self):
        marker = scope.set("inside-scene")
        a = Circle(0.5)
        b = Square(0.5).shift(2 * RIGHT)
        self.add(a, b)
        observations = []
        def first(m, dt):
            assert scope.get() == "inside-scene"
            m.shift(dt * UP)
            m.set_fill(BLUE, opacity=0.5)
            observations.append(["first", self.time, dt, *m.get_center()])
        def second(m, dt):
            assert scope.get() == "inside-scene"
            m.move_to(a.get_center() + 2 * RIGHT)
            close(m.get_center(), a.get_center() + 2 * RIGHT)
            observations.append(["second", self.time, dt, *m.get_center()])
        a.add_updater(first)
        b.add_updater(second)
        try:
            await self.wait(0.5)
        finally:
            scope.reset(marker)
        close(a.get_center(), (0, 0.5))
        close(b.get_center(), (2, 0.5))
        assert abs(a.get_fill_opacity() - 0.5) < 1e-6
        assert [v[0] for v in observations] == ["first", "second"] * (len(observations) // 2)
        a.clear_updaters(); b.clear_updaters()
        before = len(observations)
        await self.play(a.animate.shift(RIGHT), run_time=0.25, rate_func=linear)
        await self.wait(0.25)
        assert len(observations) == before
        report("callbacks", observations + [["final", self.time, *a.get_center(), *b.get_center()]])
