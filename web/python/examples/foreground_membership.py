from noon import *


class ForegroundMembership(Scene):
    def construct(self):
        red = Square(1.8, fill_color=RED, fill_opacity=1).shift(0.7 * LEFT)
        blue = Square(1.8, fill_color=BLUE, fill_opacity=1).shift(0.7 * RIGHT)
        green = Square(2.1, fill_color=GREEN, fill_opacity=1)
        later = Circle(0.8, fill_color=YELLOW, fill_opacity=1)
        family = VGroup(red, blue)

        self.add_foreground_mobjects(family)
        assert self.mobjects == [family]
        assert self.foreground_mobjects == [family]
        self.wait(0.35)

        self.play(FadeIn(green), run_time=0.35, rate_func=linear)
        assert self.mobjects == [green, family]
        assert self.foreground_mobjects == [family]

        self.remove_foreground_mobject(red)
        assert self.mobjects == [green, family]
        assert self.foreground_mobjects == [blue]
        self.wait(0.35)

        self.play(Create(later), run_time=0.35, rate_func=linear)
        assert self.mobjects == [green, red, later, blue]
        assert self.foreground_mobjects == [blue]

        self.remove_foreground_mobject(blue)
        assert self.mobjects == [green, red, later, blue]
        assert self.foreground_mobjects == []
        self.wait(0.35)
