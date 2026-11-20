from noon import Circle, LEFT, MobjectTable, RIGHT, Scene, Square, UP, VGroup, linear


class ShiftLabeledTable(Scene):
    def construct(self):
        entry = Square(side_length=0.36)
        row_label = VGroup(Circle(radius=0.12), Square(side_length=0.16).shift(RIGHT * 0.3))
        table = MobjectTable(
            [[entry, Circle(radius=0.22)]], row_labels=[row_label],
            h_buff=0.7, v_buff=0.45, include_outer_lines=True,
        )
        table.shift(LEFT * 1.5)
        self.play(table.animate.shift(RIGHT * 1.5), run_time=1, rate_func=linear)
        self.play(table.animate.shift(UP), run_time=1, rate_func=linear)
        assert table.get_entries_without_labels((1, 1)) is entry
        assert table.get_row_labels()[0] is row_label
