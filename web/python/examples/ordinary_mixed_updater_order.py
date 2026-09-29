"""Native and Python effects retain declaration order in one published frame.

The first Python updater sees native A, writes C, and creates one pending shape.
Native C then restores its bound position. The second Python updater observes
that result and the same provisional shape, without a half-published frame.
"""

from noon import Circle, Color, RIGHT, Scene, ValueTracker


class OrdinaryMixedUpdaterOrder(Scene):
    async def construct(self):
        first = Circle(radius=0.3)
        second = Circle(radius=0.3)
        self.add(first, second)
        self.bind_position(first, ValueTracker(1.0), direction=RIGHT)
        pending = []
        writes = 0
        reads = 0

        def write_after_native(mobject, _dt):
            nonlocal writes
            assert abs(mobject.get_center().x - 1.0) < 1e-6
            second.move_to((-2.0, 0.0, 0.0))
            assert abs(second.get_center().x + 2.0) < 1e-6
            if not pending:
                created = Circle(radius=0.15).shift((0.0, 1.0, 0.0))
                self.add(created)
                pending.append(created)
            writes += 1

        first.add_updater(write_after_native)
        self.bind_position(second, ValueTracker(3.0), direction=RIGHT)

        def read_after_native(mobject, _dt):
            nonlocal reads
            assert abs(mobject.get_center().x - 3.0) < 1e-6
            assert abs(pending[0].get_center().y - 1.0) < 1e-6
            pending[0].set_fill(Color(0.2, 0.8, 0.4), opacity=1.0)
            assert pending[0] in self.mobjects
            reads += 1
            assert reads == writes

        second.add_updater(read_after_native)
        await self.wait(0.25)
        assert reads == writes and reads >= 2
        assert len(self.mobjects) == 3
        assert abs(pending[0].get_center().y - 1.0) < 1e-6
