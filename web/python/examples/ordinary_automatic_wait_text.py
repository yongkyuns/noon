"""An empty initial wait followed by live Text construction and fading."""

from noon import FadeIn, FadeOut, Scene, Text, linear


class OrdinaryAutomaticWaitText(Scene):
    def construct(self):
        self.wait(0.5)

        label = Text("LATE", font_size=64)
        assert label not in self.mobjects
        self.play(FadeIn(label), run_time=1.0, rate_func=linear)
        assert label in self.mobjects
        self.play(FadeOut(label), run_time=0.5, rate_func=linear)
        assert label not in self.mobjects
        self.wait(0.25)
