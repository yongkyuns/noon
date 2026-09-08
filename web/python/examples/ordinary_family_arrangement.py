from noon import *


class OrdinaryFamilyArrangement(Scene):
    def construct(self):
        first = Circle(radius=0.2).set_fill(BLUE, opacity=1).set_stroke(width=0)
        second = Circle(radius=0.2).set_fill(YELLOW, opacity=1).set_stroke(width=0)
        second.shift(2 * RIGHT)
        # One semantic leaf participates in both a direct and a nested member.
        family = VGroup(first, VGroup(first, second))
        assert abs(family.width - 2.4) < 1e-6
        assert abs(family.height - 0.4) < 1e-6
        family.arrange(RIGHT, buff=0.2)
        self.add(first, second)
        self.wait(0.5)
        family.arrange(UP, buff=0.3, center=False)
        self.wait(0.5)
