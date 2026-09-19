from manim import *

class SampleSpaceExample(Scene):
    def construct(self):
        horizontal = SampleSpace(width=2.8, height=1.8)
        horizontal.divide_horizontally([0.25, 0.5], colors=[GREEN_E, BLUE_E])
        horizontal.shift(LEFT * 2)
        vertical = SampleSpace(width=2.8, height=1.8)
        vertical.divide_vertically([0.4, 0.35], colors=["#EC92AB", YELLOW])
        vertical.shift(RIGHT * 2)
        self.add(horizontal, vertical)
        self.wait(0.2)
