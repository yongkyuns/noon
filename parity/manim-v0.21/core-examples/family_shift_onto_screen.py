from manim import *


class OrdinaryFamilyShiftOntoScreen(Scene):
    def construct(self):
        family = Group(
            Circle(radius=0.5, color=RED).shift(RIGHT * 10 + UP * 6),
            Group(Square(side_length=0.75, color=BLUE).shift(RIGHT * 12 + UP * 5)),
        )
        self.add(family)
        returned = family.shift_onto_screen(buff=0.3)
        assert returned is family
        assert not family.is_off_screen()
        self.wait(0.2)
