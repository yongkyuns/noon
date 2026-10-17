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
