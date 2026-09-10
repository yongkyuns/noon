from noon import *


class OrdinaryTextSourceParts(Scene):
    def construct(self):
        label = Text("Noon é Noon", font_size=36)
        parts = label.source_parts_for("Noon")
        assert [(part.source_start, part.source_end) for part in parts] == [(0, 4), (8, 12)]
        label.shift(UP).set_color(BLUE)
        assert label.source_parts_for("Noon") == parts
        count = Text(f"{len(parts)} source matches", font_size=28).shift(DOWN)
        self.add(label, count)
        self.wait(0.2)
