from manim import *


class BackEasings(Scene):
    def construct(self):
        rates = [rate_functions.ease_in_back, rate_functions.ease_out_back,
                 rate_functions.ease_in_out_back]
        shapes = [Square(side_length=0.65, color=color, fill_opacity=1, stroke_width=0)
                  .shift(2 * LEFT + (2 - index) * UP)
                  for index, color in enumerate([PURE_RED, PURE_GREEN, PURE_BLUE])]
        self.add(*shapes)
        self.play(AnimationGroup(
            *(shape.animate(run_time=1, rate_func=rate).shift(4 * RIGHT)
              for shape, rate in zip(shapes, rates)),
            rate_func=linear,
        ))
