from noon import *


class NativeTextScene(Scene):
    def construct(self):
        self.label = Text("Native text", font="DejaVu Sans Mono", font_size=30)
        self.add(self.label)
        parts = self.label.source_parts_for("text")
        assert [(p.source_start, p.source_end) for p in parts] == [(7, 11)]
        self.wait(.1)
        self.other = MarkupText('<span foreground="#ff0000">Red</span>',
                                font="DejaVu Sans Mono", font_size=30)
        self.other.move_to(2 * RIGHT)
        self.add(self.other)
        self.label.shift(RIGHT)
        assert self.label.source_parts_for("text") == parts
        self.wait(.1)
