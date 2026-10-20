from manim import *


class OrdinaryFamilySpaceOutSubmobjects(Scene):
    def construct(self):
        left = Circle(radius=0.4, color=RED).shift(LEFT)
        right = Square(side_length=0.8, color=BLUE).shift(RIGHT)
        family = Group(left, Group(right))
        self.add(family)
        returned = family.space_out_submobjects(1.5)
        assert returned is family
        self.wait(0.2)
