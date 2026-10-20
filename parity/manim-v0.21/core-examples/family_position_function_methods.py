from manim import *


class OrdinaryFamilyPositionFunctionMethods(Scene):
    def construct(self):
        left = Circle(radius=0.4, color=RED).shift(LEFT)
        right = Square(side_length=0.8, color=BLUE).shift(RIGHT)
        family = Group(left, Group(right))
        self.add(family)

        def translate(point):
            return (point[0] + 0.5, point[1] - 0.25, point[2])

        returned = family.apply_function_to_position(translate)
        family.apply_function_to_submobject_positions(translate)
        assert returned is family
        self.wait(0.2)
