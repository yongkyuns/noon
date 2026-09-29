"""Async callback membership edits share one Rust publication with effective writes.

The updater removes and re-appends one already-bound Circle, catches a rejected
second operation, and shifts its callback target.  Rust keeps the successful
prefix transaction-local until the callback's single publication succeeds.
"""

import _manim_updaters
from _noon_errors import NoonValueError
from noon import Circle, Color, Scene


class OrdinaryCallbackMembershipAtomic(Scene):
    async def construct(self):
        left = Circle(radius=0.35).set_fill(Color(0.1, 0.4, 1.0), opacity=1.0)
        middle = Circle(radius=0.35).set_fill(Color(0.2, 0.8, 0.4), opacity=1.0)
        right = Circle(radius=0.35).set_fill(Color(1.0, 0.3, 0.2), opacity=1.0)
        left.shift((-1.0, 0.0, 0.0))
        right.shift((1.0, 0.0, 0.0))
        self.add(left, middle, right)
        staged = False

        def reorder_and_shift(mobject, _dt):
            nonlocal staged
            if staged:
                return
            staged = True
            assert _manim_updaters._canonical_callback_time(mobject) == 0.0
            self.remove(middle)
            self.add(middle)
            try:
                self.remove(right, right)
            except NoonValueError as error:
                assert error.category == "invalid_input"
                assert error.code == "membership.duplicate_target"
            else:
                raise AssertionError("duplicate callback removal was accepted")
            mobject.shift((0.5, 0.0, 0.0))

        left.add_updater(reorder_and_shift)
        await self.wait(0.25)
        assert staged
        assert self.mobjects == [left, right, middle]
        assert middle._scene is self
        assert abs(left.get_center().x + 0.5) < 1e-6
        assert self.time == 0.25
