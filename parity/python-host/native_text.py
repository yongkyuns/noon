from noon import *


class NativeTextScene(Scene):
    def construct(self):
        self.label = Text("Native text", font="DejaVu Sans Mono", font_size=30)
        self.add(self.label)
        self.wait(.1)
        self.other = MarkupText('<span foreground="#ff0000">Red</span>',
                                font="DejaVu Sans Mono", font_size=30)
        self.other.move_to(2 * RIGHT)
        self.add(self.other)
        self.label.shift(RIGHT)
        self.wait(.1)
