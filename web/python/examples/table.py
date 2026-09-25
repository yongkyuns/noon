"""Paired with Noon’s retained native/direct-WASM Table example."""
from noon import *


class RetainedTable(Scene):
    async def construct(self):
        await prepare_latex()
        plain = Table([["plain", "native"], ["text", "table"]], include_outer_lines=True)
        plain.shift(LEFT * 3 + UP * 1.4)
        math = MathTable([["x^2", "y"], [r"\alpha", r"\frac{1}{2}"]], include_outer_lines=True)
        math.shift(RIGHT * 3 + UP * 1.4)
        pair = VGroup(MathTex("x"), MathTex("+"))
        pair.submobjects[1].shift(RIGHT * .5)
        composite = MobjectTable([[pair, MathTex("y")]], include_outer_lines=True)
        composite.shift(DOWN * 2)
        self.add(plain, math, composite)
