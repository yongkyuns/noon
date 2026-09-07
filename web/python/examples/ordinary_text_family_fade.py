"""Plain-Text family fades composed with a disjoint shared glyph Write."""

from noon import FadeIn, FadeOut, LEFT, RIGHT, Text, UP, VGroup, Write, Scene, linear


class OrdinaryTextFamilyFade(Scene):
    def construct(self):
        left = Text("LEFT").shift(2 * LEFT + 0.5 * UP)
        right = Text("RIGHT").shift(RIGHT + 0.5 * UP)
        family = VGroup(left, right)
        writing = Text("WRITE").shift(LEFT - UP)

        self.play(
            FadeIn(family, lag_ratio=0.25),
            Write(writing),
            run_time=2.0,
            rate_func=linear,
        )
        assert family in self.mobjects
        assert writing in self.mobjects

        self.play(
            FadeOut(family, lag_ratio=0.25),
            run_time=1.0,
            rate_func=linear,
        )
        assert family not in self.mobjects
        assert writing in self.mobjects
        self.wait(0.25)
