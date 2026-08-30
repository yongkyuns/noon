from manim import *


class ShrinkToCenterExample(Scene):
    def construct(self):
        self.play(ShrinkToCenter(Text("Hello World!")))
