"""Introduce a nested diagram and glowing underline, then move the underline between steps.

The paired Rust example uses the same families and shared animation compiler.
"""
from noon import *


class GroupAnimation(Scene):
    def construct(self):
        def lines(segments):
            return [Line([*start, 0], [*end, 0], color=BLUE)
                    for start, end in segments]

        def step(x, label):
            card = Rectangle(width=2, height=1.2, color=BLUE).shift(x * RIGHT)
            number = Text(label, color=BLUE, font_size=36).shift(x * RIGHT)
            return VGroup(card, number)

        beam = Rectangle(width=0.72, height=0.12, fill=ORANGE, fill_opacity=1,
                         stroke=None).shift(1.56 * LEFT + 0.49 * DOWN)
        tip = Circle(radius=0.12, fill=YELLOW, fill_opacity=1,
                     stroke=None).shift(1.24 * LEFT + 0.49 * DOWN)
        callout = VGroup(beam, tip).set_glow(radius=Pixels(10), intensity=1.4, color=ORANGE)

        first = step(-1.8, "1")
        second = step(1.8, "2")
        connector = VGroup(*lines([
            ((-0.6, 0), (0.6, 0)),
            ((0.35, 0.2), (0.6, 0)),
            ((0.35, -0.2), (0.6, 0)),
        ]))
        diagram = VGroup(first, connector, second)
        rail = VGroup(Line([-2.4, -1.4, 0], [2.4, -1.4, 0], color=BLUE),
                      *(Line([x, -1.6, 0], [x, -1.2, 0], color=BLUE)
                        for x in (-2.4, -0.8, 0.8, 2.4)))
        # Each group has its own pivot; the marker advances between rail ticks.
        marker = Circle(radius=0.16, color=ORANGE, fill_opacity=1).shift(
            2.4 * LEFT + 1.4 * DOWN)
        self.add(rail, marker)
        self.wait(0.75)
        # The diagram and the glowing underline enter at the start of play.
        self.play(diagram.animate(lag_ratio=0.08).shift(0.5 * UP).scale(0.8),
                  rail.animate.scale(0.8),
                  marker.animate.shift(1.76 * RIGHT),
                  callout.animate.shift(0.25 * UP), run_time=1.25)
        self.wait(0.5)
        self.play(diagram.animate.rotate(-0.25),
                  rail.animate.shift(0.25 * DOWN),
                  marker.animate.shift(1.28 * RIGHT + 0.25 * DOWN), run_time=1.25)
        self.wait(0.5)
        self.play(diagram.animate.rotate(0.25).scale(1.25)
                  .shift(0.5 * DOWN).set_color(ORANGE),
                  rail.animate.scale(1.25).shift(0.25 * UP),
                  callout.animate.shift(3.32 * RIGHT + 0.58 * DOWN).scale(1.2),
                  marker.animate.shift(1.76 * RIGHT + 0.25 * UP).set_color(BLUE),
                  run_time=1.25)
        self.wait(1.0)
