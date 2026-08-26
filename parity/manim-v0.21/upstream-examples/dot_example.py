from manim import *


class DotExample(Scene):
    def construct(self):
        dot1 = Dot(point=LEFT, radius=0.08)
        dot2 = Dot(point=ORIGIN)
        dot3 = Dot(point=RIGHT)
        self.add(dot1,dot2,dot3)
