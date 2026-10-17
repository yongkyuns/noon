"""Paired with Noon’s retained native/direct-WASM Table example."""
from noon import *


class TableLabelsExample(Scene):
    def construct(self):
        row_labels = [Circle(radius=0.15), Square(side_length=0.4)]
        column_labels = [Circle(radius=0.25)]
        top_left_entry = Square(side_length=0.3)
        table = Table(
            [[" "], [" "]],
            row_labels=row_labels,
            col_labels=column_labels,
            top_left_entry=top_left_entry,
            h_buff=0.7,
            v_buff=0.45,
            include_outer_lines=True,
        )
        assert table.row_labels[0] is row_labels[0]
        assert table.row_labels[1] is row_labels[1]
        assert table.col_labels[0] is column_labels[0]
        assert table.top_left_entry is top_left_entry
        assert table.submobjects[3].submobjects[0] is row_labels[0]
        assert table.submobjects[4].submobjects[0] is column_labels[0]
        assert table.submobjects[5].submobjects[0] is top_left_entry
        copied = table.copy()
        assert copied.row_labels[0] is not row_labels[0]
        assert copied.col_labels[0] is not column_labels[0]
        assert copied.top_left_entry is not top_left_entry
        self.add(table)
        self.wait(0.2)


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


class MobjectTableFamilyLabelsExample(Scene):
    def construct(self):
        row_labels = [
            VGroup(Circle(radius=0.12), Square(side_length=0.16).shift(RIGHT * 0.3)),
            VGroup(Square(side_length=0.14), Circle(radius=0.1).shift(RIGHT * 0.28)),
        ]
        col_labels = [
            VGroup(Circle(radius=0.11), Square(side_length=0.13).shift(RIGHT * 0.26)),
            VGroup(Square(side_length=0.12), Circle(radius=0.09).shift(RIGHT * 0.24)),
        ]
        top_left_entry = VGroup(
            Circle(radius=0.18), Square(side_length=0.12).shift(RIGHT * 0.32)
        )
        table = MobjectTable(
            [[Circle(radius=0.3), Square(side_length=0.42)],
             [Square(side_length=0.36), Circle(radius=0.22)]],
            row_labels=row_labels,
            col_labels=col_labels,
            top_left_entry=top_left_entry,
            h_buff=0.7,
            v_buff=0.45,
            include_outer_lines=True,
        )
        self.add(table)
        self.wait(0.2)

# The compatibility gallery executes this file as its single Table + MathTable example.
# Keep the other scenes above available to parity tools without making selection ambiguous.
result = RetainedTable()
