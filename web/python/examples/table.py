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
        row_label = Text("r")
        column_labels = [Text("a"), Text("b")]
        top_left_entry = Text("i")
        composite = MobjectTable(
            [[pair, MathTex("y")]],
            row_labels=[row_label],
            col_labels=column_labels,
            top_left_entry=top_left_entry,
            include_outer_lines=True,
            include_inner_lines=False,
            line_config={"color": YELLOW, "stroke_width": 6, "stroke_opacity": 0.75},
        )
        composite.shift(DOWN * 2)
        assert composite.get_row_labels().submobjects == [row_label]
        assert composite.get_col_labels().submobjects == column_labels
        assert composite.get_labels().submobjects == [top_left_entry, *column_labels, row_label]
        self.add(plain, math, composite)
