from noon import *


class VariableExample(Scene):
    requires_latex = True

    def construct(self):
        variable = Variable(
            12_345.6,
            "x",
            num_decimal_places=2,
        )
        self.add(variable)
