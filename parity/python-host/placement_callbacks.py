"""Source-side move_to consumes settled updater state on CPython and Pyodide."""
from noon import Scene, Square, RIGHT, UP
import json


def close(actual, expected):
    if len(actual) != len(expected) or any(abs(a - b) > 2e-5 for a, b in zip(actual, expected)):
        raise AssertionError((actual, expected))


class PlacementCallbacks(Scene):
    async def construct(self):
        marker = Square(side_length=0.75)
        self.add(marker)
        calls = []

        def first(m, dt):
            calls.append(["first", self.time, dt, *m.get_center()])
            m.shift(dt * RIGHT)

        def second(m, dt):
            calls.append(["second", self.time, dt, *m.get_center()])
            m.shift(dt * UP)

        marker.add_updater(first)
        marker.add_updater(second)
        await self.wait(0.25)
        close(marker.get_center(), (0.25, 0.25))
        count = len(calls)
        # Reset to the original authored coordinate, then repeat at the same time.
        # The first operation must work even when its semantic write is a no-op.
        marker.move_to([0, 0, 0])
        close(marker.get_center(), (0, 0))
        marker.move_to([0.5, 1, 0])
        close(marker.get_center(), (0.5, 1))
        if len(calls) != count or self.time != 0.25 or len(marker.get_updaters()) != 2:
            raise AssertionError("placement invoked, removed or advanced an updater")
        await self.wait(0.25)
        close(marker.get_center(), (0.75, 1.25))
        marker.move_to([2, -1, 0])
        close(marker.get_center(), (2, -1))
        marker.clear_updaters()
        count = len(calls)
        await self.wait(0.25)
        close(marker.get_center(), (2, -1))
        if len(calls) != count:
            raise AssertionError("released updater ran again")
        print("NOON_HOST_REPORT " + json.dumps({
            "case": "placement_callbacks",
            "observations": calls + [["final", self.time, *marker.get_center()]],
        }, allow_nan=False))
