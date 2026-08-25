from noon import *


class HostUpdaterPerfScene(Scene):
    def construct(self):
        anchor = Circle(radius=0.4, color=RED).shift(RIGHT * 2)
        follower = Square(side_length=0.5, color=BLUE).shift(LEFT)

        def removed(mobject):
            mobject.shift(LEFT * 99)

        def follow(mobject, dt):
            # Deliberately read another mobject so the callback phase must expose
            # a coherent scene snapshot, then emit both transform and style work.
            mobject.move_to(anchor.get_center() + UP * dt)
            mobject.set_opacity(0.5 + dt)

        follower.add_updater(removed)
        follower.remove_updater(removed)
        follower.add_updater(follow)
        assert follower.has_updaters()
        assert follower.get_updaters() == [follow]
        self.add(anchor, follower)
