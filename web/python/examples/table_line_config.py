from noon import *


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
