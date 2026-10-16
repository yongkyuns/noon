from manim import *


class ElasticBounceEasings(Scene):
    def construct(self):
        rates = [rate_functions.ease_in_elastic, rate_functions.ease_out_elastic,
                 rate_functions.ease_in_out_elastic, rate_functions.ease_in_bounce,
                 rate_functions.ease_out_bounce, rate_functions.ease_in_out_bounce]
        shapes = [Square(side_length=0.45, color=color, fill_opacity=1, stroke_width=0)
                  .shift(2 * LEFT + (2 - index * .6) * UP)
                  for index, color in enumerate([PURE_RED, PURE_GREEN, PURE_BLUE,
                                               PURE_CYAN, PURE_MAGENTA, PURE_RED])]
        self.add(*shapes)
        self.play(AnimationGroup(
            *(shape.animate(run_time=1, rate_func=rate).shift(4 * RIGHT)
              for shape, rate in zip(shapes, rates)),
            rate_func=linear,
        ))
