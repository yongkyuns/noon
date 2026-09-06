"""Paired scaled and translated FadeIn/FadeOut over shared Rust semantics."""

from noon import BLUE, RIGHT, Circle, FadeIn, FadeOut, Scene, linear


class OrdinaryAffineFade(Scene):
    def construct(self):
        circle = (
            Circle(radius=0.4)
            .set_fill(BLUE, opacity=1.0)
            .set_stroke(opacity=0.0)
        )

        self.play(
            FadeIn(circle, shift=2 * RIGHT, scale=0.25),
            run_time=1.0,
            rate_func=linear,
        )
        assert tuple(circle.get_center()) == (0.0, 0.0)

        self.play(
            FadeOut(circle, target_position=2 * RIGHT, scale=0.15),
            run_time=1.0,
            rate_func=linear,
        )
        assert circle._scene is None
