"""Separate updaters publish two retained paths and one analytic row together.

The witness updater writes first. All three content updaters observe that same
ordered overlay before their terminal content outputs. Two ordinary waits make
the changed and unchanged path samples observable at exact segment boundaries.
"""

from _manim_updaters import _canonical_callback_time
from noon import Circle, Color, Scene


class OrdinaryEffectiveMixedContentUpdaters(Scene):
    async def construct(self):
        def shape(color, position):
            return (
                Circle(radius=0.1)
                .set_fill(color, opacity=1.0)
                .set_stroke(width=0.0)
                .shift(position)
            )

        witness = shape(Color(1.0, 1.0, 0.0), (0.25, -1.5, 0.0))
        first = shape(Color(1.0, 0.0, 0.0), (-2.0, 0.0, 0.0))
        second = shape(Color(0.0, 0.0, 1.0), (0.0, 0.0, 0.0))
        analytic = shape(Color(0.0, 1.0, 0.0), (2.0, 0.0, 0.0))
        self.add(witness, first, second, analytic)
        identities = [item.id for item in self.mobjects]
        phases = []

        def close(actual, expected):
            assert abs(actual - expected) < 1e-6, (actual, expected)

        def center(item, x, y):
            actual = item.get_center()
            close(actual[0], x)
            close(actual[1], y)

        def begin_phase(mobject, _dt):
            time = _canonical_callback_time(mobject)
            if phases:
                assert phases[-1]["order"] == ["witness", "path_a", "path_b", "circle"]
                assert time >= phases[-1]["time"]
                # These reads observe the preceding committed content version.
                center(first, -2.0, 0.0)
                center(second, 0.0, 0.0)
                center(analytic, 2.0, 0.0)
            phases.append({"time": time, "order": ["witness"]})
            mobject.move_to((0.25 + time, -1.5, 0.0))
            center(mobject, 0.25 + time, -1.5)

        def observe_witness(mobject, expected_order):
            time = _canonical_callback_time(mobject)
            assert phases[-1]["time"] == time
            assert phases[-1]["order"] == expected_order
            # A later callable must see the earlier callable's unpublished write.
            center(witness, 0.25 + time, -1.5)
            return time

        def path_a(mobject, _dt):
            time = observe_witness(mobject, ["witness"])
            width = 0.4 if time < 0.125 else 0.8
            mobject.set_effective_path(
                [(-width, -0.3), (width, -0.3), (width, 0.3), (-width, 0.3)],
                closed=True,
            )
            phases[-1]["order"].append("path_a")

        def path_b(mobject, _dt):
            time = observe_witness(mobject, ["witness", "path_a"])
            height = 0.3 if time < 0.125 else 0.6
            mobject.set_effective_path(
                [(-0.3, 0.0), (0.0, height), (0.3, 0.0), (0.0, -height)],
                closed=True,
            )
            phases[-1]["order"].append("path_b")

        def circle(mobject, _dt):
            time = observe_witness(mobject, ["witness", "path_a", "path_b"])
            mobject.set_effective_circle(0.2 if time < 0.125 else 0.4)
            phases[-1]["order"].append("circle")

        witness.add_updater(begin_phase)
        first.add_updater(path_a)
        second.add_updater(path_b)
        analytic.add_updater(circle)

        def check_committed(time):
            assert phases[0]["time"] == 0.0
            assert phases[-1]["time"] == time
            assert phases[-1]["order"] == ["witness", "path_a", "path_b", "circle"]
            assert [item.id for item in self.mobjects] == identities
            center(witness, 0.25 + time, -1.5)
            center(first, -2.0, 0.0)
            center(second, 0.0, 0.0)
            center(analytic, 2.0, 0.0)
            for item, width, height in [
                (first, 1.6, 0.6), (second, 0.6, 1.2), (analytic, 0.8, 0.8),
            ]:
                close(item.width, width)
                close(item.height, height)

        await self.wait(0.125)
        check_committed(0.125)
        await self.wait(0.125)
        check_committed(0.25)
        # The second boundary must invoke a new phase with value-equal geometry;
        # the content batch cannot be skipped merely because paths are unchanged.
        assert {0.0, 0.125, 0.25}.issubset({phase["time"] for phase in phases})
