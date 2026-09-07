"""An empty initial wait followed by live Text and Typst construction."""

from noon import DOWN, FadeIn, FadeOut, MathTypst, Scene, Text, Typst, UP, linear


class OrdinaryAutomaticWaitText(Scene):
    def construct(self):
        self.wait(0.5)

        late_objects = [
            Text("LATE", font_size=56).shift(2 * UP),
            Typst("*Typst*", font_size=48),
            MathTypst("x^2 + y^2", font_size=48).shift(2 * DOWN),
        ]
        assert all(obj not in self.mobjects for obj in late_objects)
        self.play(*(FadeIn(obj) for obj in late_objects), run_time=1.0, rate_func=linear)
        assert all(obj in self.mobjects for obj in late_objects)
        self.play(*(FadeOut(obj) for obj in late_objects), run_time=0.5, rate_func=linear)
        assert all(obj not in self.mobjects for obj in late_objects)
        self.wait(0.25)
