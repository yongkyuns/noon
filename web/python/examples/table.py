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
        composite = MobjectTable(
            [[pair, MathTex("y")]],
            row_labels=[Text("r")],
            col_labels=[Text("a"), Text("b")],
            top_left_entry=Text("i"),
            include_outer_lines=True,
            include_inner_lines=False,
            line_config={"color": YELLOW, "stroke_width": 6, "stroke_opacity": 0.75},
        )
        composite.shift(DOWN * 2)
        self.add(plain, math, composite)


class TableInnerLinesOmittedExample(Scene):
    def construct(self):
        table = MobjectTable(
            [[Circle(radius=0.3), Square(side_length=0.5)],
             [Square(side_length=0.4), Circle(radius=0.2)]],
            h_buff=0.7,
            v_buff=0.45,
            include_outer_lines=True,
            include_inner_lines=False,
        )
        self.add(table)
        self.wait(0.2)


class TableLineConfigExample(Scene):
    def construct(self):
        table = MobjectTable(
            [[Circle(radius=0.3), Square(side_length=0.5)],
             [Square(side_length=0.4), Circle(radius=0.2)]],
            h_buff=0.7,
            v_buff=0.45,
            include_outer_lines=True,
            line_config={"color": RED, "stroke_width": 6, "stroke_opacity": 0.6},
        )
        self.add(table)
        self.wait(0.2)
