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
        assert len(table.get_entries()) == 6
        assert table.get_entries((1, 1)) is top_left_entry
        assert table.get_entries((1, 2)) is column_labels[0]
        assert table.get_entries((2, 1)) is row_labels[0]
        assert table.get_entries((2, 2)) is table.get_entries_without_labels((1, 1))
        assert len(table.get_entries_without_labels()) == 2
        assert len(table.get_rows()) == 3
        assert len(table.get_columns()) == 2
        self.add(table)
        self.wait(0.2)
