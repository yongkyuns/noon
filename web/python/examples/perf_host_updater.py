from noon import *


class HostUpdaterPerfScene(Scene):
    def construct(self):
        anchor = Circle(radius=0.4, color=RED).shift(LEFT * 2)
        follower = Square(side_length=0.5, color=BLUE).shift(RIGHT * 2)

        def removed(mobject):
            mobject.shift(LEFT * 99)

        def follow(mobject, dt):
            # Read the Rust-published target row, then emit both transform and
            # style work. The unrelated anchor must never enter the sparse phase.
            center = mobject.get_center()
            mobject.move_to((center.x, dt, 0.0))
            mobject.set_object_opacity(0.5 + dt)

        follower.add_updater(removed)
        follower.remove_updater(removed)
        follower.add_updater(follow)
        assert follower.has_updaters()
        assert follower.get_updaters() == [follow]
        self.add(anchor, follower)

        # Bootstrap the same canonical session later leased to the execution
        # worker. The wait supplies a finite forward interval; Rust still owns
        # callback activation, ordering, delta time, and publication.
        live = self.live_execution()
        assert live.wait(1.0) == 1.0

        # Live registration is supported after completion, never while this
        # callback-bearing segment is still pending. Rejection must roll back.
        def during_pending_segment(mobject):
            mobject.shift(RIGHT * 100)

        try:
            follower.add_updater(during_pending_segment)
        except Exception as error:
            assert "complete the current live segment before continuing" in str(error)
        else:
            raise AssertionError("registration during a pending segment was silently accepted")
        assert follower.get_updaters() == [follow]
