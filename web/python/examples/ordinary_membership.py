from noon import *


class OrdinaryMembership(Scene):
    def construct(self):
        red = Square(2, fill_color=RED, fill_opacity=1).shift(0.5 * LEFT)
        blue = Square(2, fill_color=BLUE, fill_opacity=1).shift(0.5 * RIGHT)
        green = Square(2, fill_color=GREEN, fill_opacity=1)
        family = VGroup(red, blue)

        try:
            self.add(red, object())
            raise AssertionError("invalid membership batch must fail")
        except (TypeError, ValueError):
            pass
        assert self.mobjects == []
        self.add(red, blue)
        assert self.mobjects == [red, blue]
        self.wait(0.5)
        self.replace(red, green)
        assert self.mobjects == [green, blue]
        self.wait(0.5)
        self.remove(blue)
        assert self.mobjects == [green]
        self.wait(0.5)
        self.add(family)
        assert self.mobjects == [green, family]
        self.wait(0.5)
        self.remove(red)
        assert self.mobjects == [green, blue]
        self.wait(0.5)
        self.add(green)
        assert self.mobjects == [blue, green]
        self.wait(0.5)
        self.clear()
        assert self.mobjects == []
        self.wait(0.5)
