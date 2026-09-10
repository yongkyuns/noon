from manim import *

class OrdinaryBooleanGeometry(Scene):
    def construct(self):
        a = Circle(radius=0.9).shift(LEFT * 0.4)
        b = Circle(radius=0.9).shift(RIGHT * 0.4)
        for operation, x, y, color in (
            (Union, -2.5, 1.5, BLUE),
            (Intersection, 2.5, 1.5, GREEN),
            (Difference, -2.5, -1.5, YELLOW),
            (Exclusion, 2.5, -1.5, RED),
        ):
            result = operation(a, b, color=color, fill_opacity=0.7, stroke_width=2)
            self.add(result.shift(RIGHT * x + UP * y))
        self.wait(0.2)
