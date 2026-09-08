from noon import *


class OrdinaryFamilyArrangement(Scene):
    def construct(self):
        first = Circle(radius=0.2).set_fill(BLUE, opacity=1).set_stroke(width=0)
        second = Circle(radius=0.2).set_fill(YELLOW, opacity=1).set_stroke(width=0)
        second.shift(2 * RIGHT)
        # One semantic leaf participates in both a direct and a nested member.
        nested = VGroup(first, second)
        family = VGroup(first, nested)
        assert abs(family.width - 2.4) < 1e-6
        assert abs(family.height - 0.4) < 1e-6
        family.arrange(RIGHT, buff=0.2)
        self.add(first, second)
        self.wait(0.5)
        family = VGroup(first, nested)
        family.remove(nested)
        family.add(nested)
        family.arrange(UP, buff=0.3, center=False)
        assert abs(nested.width - 3.7) < 1e-5
        nested.move_to(ORIGIN, coor_mask=(1, 0, 0))
        nested.next_to(ORIGIN, direction=2 * UP, buff=0.25, coor_mask=(0, 1, 0))
        nested.align_to(first, UP)
        self.wait(0.5)
