from noon import *


class MixedFamilyFade(Scene):
    def construct(self):
        family = VGroup(Circle(radius=0.4).shift(2 * LEFT),
                        Square(side_length=0.8), Text("TEXT").shift(2 * RIGHT))
        self.play(FadeIn(family, lag_ratio=0.25), run_time=1, rate_func=linear)
        assert family in self.mobjects
        self.play(FadeOut(family, lag_ratio=0.25), run_time=1, rate_func=linear)
        assert family not in self.mobjects
        self.wait(0.25)
