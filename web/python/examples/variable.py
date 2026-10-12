from noon import *


class VariableExample(Scene):
    async def construct(self):
        await prepare_latex()
        variable = Variable(
            12_345.6,
            "x",
            num_decimal_places=2,
        )
        self.add(variable)
