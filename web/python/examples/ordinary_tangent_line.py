"""Source-equivalent TangentLine example using shared Rust path sampling."""
from noon import *


class TangentLineExample(Scene):
    def construct(self):
        circle = Circle(radius=2)
        line_1 = TangentLine(circle, alpha=0.0, length=4, color=BLUE_D)
        line_2 = TangentLine(circle, alpha=0.4, length=4, color=GREEN)
        self.add(circle, line_1, line_2)
        self.wait(0.2)
