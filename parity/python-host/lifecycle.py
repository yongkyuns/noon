from noon import *
import gc
import json
import weakref

def close(actual, expected, tolerance=2e-5):
    assert len(actual) == len(expected), (actual, expected)
    assert all(abs(a - b) <= tolerance for a, b in zip(actual, expected)), (actual, expected)

def report(case, observations):
    print("NOON_HOST_REPORT " + json.dumps({"case": case, "observations": observations}, allow_nan=False))

def abandoned_updater_ref():
    temporary = Circle(0.1)
    temporary.add_updater(lambda obj, dt: None)
    return weakref.ref(temporary)

class Lifecycle(Scene):
    async def construct(self):
        # This is a conformance-only collection point, not runtime GC policy.
        # An unbound wrapper has no owning session to release its updater.
        abandoned = abandoned_updater_ref()
        gc.collect()
        assert abandoned() is None, "updater discovery owns an abandoned wrapper"
        a = Square(0.5).shift(LEFT)
        self.add(a)
        identity = a._semantic_handle
        await self.play(Transform(a, Circle(0.75).shift(RIGHT)), run_time=0.25, rate_func=linear)
        close(a.get_center(), (1, 0))
        assert a._semantic_handle is identity
        await self.play(FadeOut(a), run_time=0.25, rate_func=linear)
        assert a not in self.mobjects
        self.add(a)
        assert a._semantic_handle is identity and a in self.mobjects
        clone = a.copy()
        clone.shift(UP)
        self.add(clone)
        await self.play(a.animate.shift(LEFT), clone.animate.shift(RIGHT), run_time=0.25, rate_func=linear)
        self.remove(a)
        self.add(a)
        await self.wait(0.25)
        close(a.get_center(), (0, 0))
        close(clone.get_center(), (2, 1))
        assert len(self.mobjects) == 2
        report("lifecycle", [[self.time, *a.get_center(), *clone.get_center()], [a._semantic_handle is identity, a is not clone]])
