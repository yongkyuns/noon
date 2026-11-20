from noon import *


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
        table.shift(LEFT * 1.5)
        entry = table.get_entries((2, 2))
        self.play(table.animate.shift(RIGHT * 1.5), run_time=1, rate_func=linear)
        self.play(table.animate.shift(UP), run_time=1, rate_func=linear)
        assert table.get_entries((2, 2)) is entry
        assert table.get_row_labels().submobjects == row_labels
