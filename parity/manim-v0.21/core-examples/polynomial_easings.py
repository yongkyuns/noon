from manim import *


class PolynomialEasings(Scene):
    def construct(self):
        colors = [PURE_RED, PURE_GREEN, PURE_BLUE, PURE_CYAN, PURE_MAGENTA]
        rates = [
            ease_in_quad,
            ease_out_quad,
            ease_in_out_quad,
            ease_in_cubic,
            ease_out_cubic,
            ease_in_out_cubic,
            ease_in_quart,
            ease_out_quart,
            ease_in_out_quart,
            ease_in_quint,
            ease_out_quint,
            ease_in_out_quint,
        ]
        shapes = []
        for index, rate in enumerate(rates):
            shape = Square(side_length=0.28, color=colors[index % len(colors)],
                           fill_opacity=1, stroke_width=0).shift(2 * LEFT + (2 - index * 0.38) * UP)
            self.add(shape)
            shapes.append(shape)
        self.play(AnimationGroup(
            *(shape.animate(run_time=1, rate_func=rate).shift(4 * RIGHT)
              for shape, rate in zip(shapes, rates)),
            rate_func=linear,
        ))
