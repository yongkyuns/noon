from manim import *


class VariableExample(Scene):
    def construct(self):
        variable = Variable(
            12_345.6,
            "x",
            num_decimal_places=2,
        )
        self.add(variable)
        self.wait(.2)
