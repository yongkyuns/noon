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
        # This progress marker moves independently of the diagram's shared pivot.
        marker = Circle(radius=0.16, color=ORANGE, fill_opacity=1).shift(
            2.4 * LEFT + 1.4 * DOWN)
        self.add(diagram, marker)
        self.wait(0.75)
        self.play(diagram.animate.shift(0.5 * UP).scale(0.8),
                  marker.animate.shift(1.6 * RIGHT), run_time=1.25)
        self.wait(0.5)
        self.play(diagram.animate.rotate(-0.25),
                  marker.animate.shift(1.6 * RIGHT), run_time=1.25)
        self.wait(0.5)
        self.play(diagram.animate.rotate(0.25).scale(1.25)
                  .shift(0.5 * DOWN).set_color(ORANGE),
                  marker.animate.shift(1.6 * RIGHT).set_color(BLUE), run_time=1.25)
        self.wait(1.0)
