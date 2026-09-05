"""Paired Python proof for ordinary affine ``Scene.play`` and ``Scene.wait``.

Each barrier runs through the one retained Rust semantic execution session.  The
resumed ``shift`` is a normal shared live mutation after endpoint completion,
and the second ordinary Transform captures that published source.
"""

from noon import Circle, Color, Scene, Transform, linear


class OrdinaryAffinePlay(Scene):
    def construct(self):
        circle = Circle(radius=0.4).set_fill(Color(0.0, 0.4, 1.0), opacity=1.0)
        self.add(circle)

        self.play(circle.animate.shift((2.0, -1.0, 0.0)), run_time=2.0, rate_func=linear)
        assert self.time == 2.0
        assert tuple(circle.get_center()) == (2.0, -1.0)

        self.wait(1.0)
        assert self.time == 3.0
        assert tuple(circle.get_center()) == (2.0, -1.0)

        # This is `LiveSession::shift` through the retained player, not a
        # Python target/state update.  The following Transform captures x=3.
        circle.shift((1.0, 0.0, 0.0))
        assert self.time == 3.0
        assert tuple(circle.get_center()) == (3.0, -1.0)

        target = circle.copy().shift((2.0, 0.0, 0.0))
        self.play(Transform(circle, target), run_time=1.0, rate_func=linear)
        assert self.time == 4.0
        assert tuple(circle.get_center()) == (5.0, -1.0)
