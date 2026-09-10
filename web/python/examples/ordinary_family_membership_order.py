from noon import *


class OrdinaryFamilyMembershipOrder(Scene):
    def construct(self):
        a = Square(side_length=2, color="#FF0000", fill_opacity=1, stroke_width=0)
        b = Square(side_length=2, color="#0000FF", fill_opacity=1, stroke_width=0).shift(RIGHT)
        family = VGroup(a, b, a)
        assert list(family) == [b, a]
        family.add(b)
        assert list(family) == [a, b]
        self.add(family)
        live = self.live_execution()
        family.add(a)
        assert list(family) == [b, a]
        live.wait(0.2)
