from noon import *


class DecimalTableFormatExample(Scene):
    async def construct(self):
        await prepare_latex()
        table = DecimalTable(
            [[1234.5, -2.25]],
            element_to_mobject_config={
                "num_decimal_places": 2,
                "include_sign": True,
                "group_with_commas": True,
            },
            include_outer_lines=True,
        )
        self.add(table)
        await self.wait(0.2)
