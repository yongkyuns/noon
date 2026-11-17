"""Move a diagram and progress rail as independent groups in one play.

The paired Rust example uses the same families and shared animation compiler.
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
        rail = VGroup(Line([-2.4, -1.4, 0], [2.4, -1.4, 0], color=BLUE),
                      *(Line([x, -1.6, 0], [x, -1.2, 0], color=BLUE)
                        for x in (-2.4, -0.8, 0.8, 2.4)))
        # Each group has its own pivot; the marker advances between rail ticks.
        marker = Circle(radius=0.16, color=ORANGE, fill_opacity=1).shift(
            2.4 * LEFT + 1.4 * DOWN)
        self.add(diagram, rail, marker)
        self.wait(0.75)
        self.play(diagram.animate.shift(0.5 * UP).scale(0.8),
                  rail.animate.scale(0.8),
                  marker.animate.shift(1.76 * RIGHT), run_time=1.25)
        self.wait(0.5)
        self.play(diagram.animate.rotate(-0.25),
                  rail.animate.shift(0.25 * DOWN),
                  marker.animate.shift(1.28 * RIGHT + 0.25 * DOWN), run_time=1.25)
        self.wait(0.5)
        self.play(diagram.animate.rotate(0.25).scale(1.25)
                  .shift(0.5 * DOWN).set_color(ORANGE),
                  rail.animate.scale(1.25).shift(0.25 * UP),
                  marker.animate.shift(1.76 * RIGHT + 0.25 * UP).set_color(BLUE),
                  run_time=1.25)
        self.wait(1.0)
