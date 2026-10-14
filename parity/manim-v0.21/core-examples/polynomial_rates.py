from manim import *

class PolynomialRates(Scene):
    def construct(self):
        shapes = [Square(side_length=0.65, color=color, fill_opacity=1, stroke_width=0)
                  .shift(LEFT * 2 + UP * (2 - index))
                  for index, color in enumerate([PURE_RED, PURE_GREEN, PURE_BLUE, PURE_CYAN, PURE_MAGENTA])]
        self.add(*shapes)
        rates = [smoothstep, smootherstep, smoothererstep, slow_into, double_smooth]
        self.play(AnimationGroup(
            *(shape.animate(run_time=1, rate_func=rate).shift(RIGHT * 4)
              for shape, rate in zip(shapes, rates)),
            rate_func=linear,
        ))
