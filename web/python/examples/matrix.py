"""Shared Matrix, IntegerMatrix, DecimalMatrix, and MobjectMatrix example."""
from noon import *


class MatrixExample(Scene):
    async def construct(self):
        await prepare_latex()
        symbols = Matrix([["a", "b"], ["c", "d"]]).shift(LEFT * 4)
        integers = IntegerMatrix([[1, -2], [3, 4]])
        decimals = DecimalMatrix([[1.5, -2.0], [3.25, 4.0]]).shift(RIGHT * 4)
        entries = [[MathTex("x"), MathTex("y")], [MathTex("z"), MathTex("w")]]
        existing = MobjectMatrix(entries).shift(DOWN * 2.5)

        assert len(symbols.get_entries()) == 4
        assert len(symbols.get_rows()) == 2
        assert len(symbols.get_columns()) == 2
        assert existing.get_entries()[0] is entries[0][0]
        self.add(symbols, integers, decimals, existing)
