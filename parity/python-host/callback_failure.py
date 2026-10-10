from noon import *
import json

def close(actual, expected, tolerance=2e-5):
    assert len(actual) == len(expected), (actual, expected)
    assert all(abs(a - b) <= tolerance for a, b in zip(actual, expected)), (actual, expected)

def report(case, observations):
    print("NOON_HOST_REPORT " + json.dumps({"case": case, "observations": observations}, allow_nan=False))

class CallbackFailure(Scene):
    def setup(self):
        self.seen = []
    async def construct(self):
        self.a = Circle(0.5)
        self.b = Square(0.5).shift(RIGHT)
        self.add(self.a, self.b)
        def first(m, dt):
            m.shift(UP)
            self.seen.append("staged")
        def last(m, dt):
            close(self.a.get_center(), (0, 1))
            self.seen.append("rejected")
            raise ValueError("intentional callback abort")
        self.a.add_updater(first)
        self.b.add_updater(last)
        try:
            await self.wait(0.5)
        except NoonCallbackError as error:
            assert error.category == "callback_failure"
            raise
        raise AssertionError("failed callback continuation resumed")
    def tear_down(self):
        # The ordered temporary overlay was visible inside the callback, but
        # not committed to the published frame when the later callback failed.
        close(self.a.get_center(), (0, 0))
        close(self.b.get_center(), (1, 0))
        assert self.seen == ["staged", "rejected"]
        report("callback_failure", [[*self.a.get_center(), *self.b.get_center()], self.seen])
