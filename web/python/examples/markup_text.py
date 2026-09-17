from noon import *


class MarkupTextExample(Scene):
    def construct(self):
        title = MarkupText(
            '<b>Noon</b> <i>markup</i> <tt>&lt;Rust&gt;</tt>\n'
            '<span foreground="#58c4dd">bold</span> and '
            '<span fgcolor="#ff862f">color</span>',
            font="DejaVu Sans Mono",
            font_size=42,
        )
        self.add(title)
        self.wait(0.2)
