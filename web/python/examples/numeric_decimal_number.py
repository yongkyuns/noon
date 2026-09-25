from noon import *


class NumericDecimalNumberExample(Scene):
    async def construct(self):
        # The optional real LaTeX backend is prepared once before authoring.
        await prepare_latex()
        number = DecimalNumber(
            -0.004,
            num_decimal_places=2,
            include_sign=True,
            group_with_commas=True,
        )
        number.set_value(12_345.6)
        assert number.get_value() == 12_345.6
        self.add(number)
