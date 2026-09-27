"""Table layout and retained entry identity against pinned ManimCE."""
from manim import *


class MathTableExample(Scene):
    def construct(self):
        table = MathTable([["x", "y"], ["1", "2"]], h_buff=0.9, v_buff=0.4,
                          include_outer_lines=True)
        table.scale(0.8).shift(LEFT * 1.2)
        table.add_highlighted_cell((1, 2))
        self.add(table)
        self.wait(0.2)


class MobjectTableExample(Scene):
    def construct(self):
        pair = VGroup(Circle(radius=0.2), Square(side_length=0.3).shift(RIGHT * 0.5))
        table = MobjectTable([[pair, Circle(radius=0.4)],
                              [Square(side_length=0.5), Circle(radius=0.25)]],
                             h_buff=0.7, v_buff=0.5, include_outer_lines=True)
        table.shift(RIGHT * 0.5)
        self.add(table)
        self.wait(0.2)


class TableHighlightOrder(Scene):
    def construct(self):
        backdrop = Rectangle(width=4, height=3, fill_color=RED,
                             fill_opacity=0.5, stroke_width=0)
        table = MobjectTable([[Square(side_length=0.8, color=BLUE, fill_opacity=1)]],
                             h_buff=0.4, v_buff=0.4, include_outer_lines=True)
        table.add_highlighted_cell((1, 1), color=GREEN)
        table.add_highlighted_cell((1, 1), color=YELLOW, fill_opacity=0.5)
        self.add(backdrop, table)
        self.wait(0.2)
