"""Keep diagram labels and connectors together while changing the composition.

The paired Rust example uses the same flat family and shared animation compiler.
"""
from noon import *


class GroupAnimation(Scene):
    def construct(self):
        parts = [Rectangle(width=2, height=1.2, color=BLUE).shift(x * RIGHT)
                 for x in (-1.8, 1.8)]
        # Connector, arrow head, and the step labels 1 / 2 are ordinary lines.
        for start, end in [
            ((-0.6, 0), (0.6, 0)),
            ((0.35, 0.2), (0.6, 0)),
            ((0.35, -0.2), (0.6, 0)),
            ((-1.8, -0.25), (-1.8, 0.25)),
            ((1.6, 0.25), (2.0, 0.25)),
            ((2.0, 0.25), (2.0, 0)),
            ((2.0, 0), (1.6, -0.25)),
            ((1.6, -0.25), (2.0, -0.25)),
        ]:
            parts.append(Line([*start, 0], [*end, 0], color=BLUE))
        diagram = VGroup(*parts)
        self.add(diagram)
        self.wait(0.75)
        self.play(diagram.animate.shift(0.5 * UP).scale(0.8), run_time=1.25)
        self.wait(0.5)
        self.play(diagram.animate.rotate(-0.25), run_time=1.25)
        self.wait(0.5)
        self.play(diagram.animate.rotate(0.25).scale(1.25)
                  .shift(0.5 * DOWN).set_color(ORANGE), run_time=1.25)
        self.wait(1.0)
