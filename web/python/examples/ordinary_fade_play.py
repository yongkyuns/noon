"""Paired ordinary FadeIn/FadeOut lifecycle proof over one Rust session.

The detached circle enters through the shared FadeIn transaction, leaves through
shared FadeOut completion, then re-enters with its original semantic handle and
wrapper ObjectId. Python owns no presence or appearance timeline.
"""

from noon import Circle, Color, FadeIn, FadeOut, Scene, linear


class OrdinaryFadePlay(Scene):
    def construct(self):
        circle = Circle(radius=0.4).set_fill(Color(0.0, 0.4, 1.0), opacity=1.0)

        self.play(FadeIn(circle), run_time=1.0, rate_func=linear)
        first_id = circle.id
        assert self.time == 1.0
        assert tuple(circle.get_center()) == (0.0, 0.0)

        self.play(FadeOut(circle), run_time=1.0, rate_func=linear)
        assert self.time == 2.0
        assert circle._scene is None

        # This routes through Rust LiveSession.add with the original semantic
        # handle and derived ObjectId; it does not allocate a replacement scene row.
        self.add(circle)
        assert circle.id == first_id
        assert circle._scene is self
