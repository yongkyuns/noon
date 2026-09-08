from noon import *


class OrdinaryFamilyPlacement(Scene):
    def construct(self):
        first = Circle(radius=0.2).set_fill(BLUE, opacity=1).set_stroke(width=0)
        second = Circle(radius=0.2).set_fill(YELLOW, opacity=1).set_stroke(width=0)
        anchor = Square(side_length=1).set_fill(RED, opacity=1).set_stroke(width=0)
        second.shift(0.8 * RIGHT)
        nested = VGroup(second)
        family = VGroup()
        family.add(first, nested)
        family.remove(nested)
        family.add(nested)
        target = VGroup(anchor)
        family.next_to(anchor, direction=2 * RIGHT, buff=0.25)
        family.align_to(UP, UP)
        family.move_to(target, coor_mask=(1, 0, 0))
        family.shift(0.2 * UP)
        self.add(first, second, anchor)
        self.wait(1)
