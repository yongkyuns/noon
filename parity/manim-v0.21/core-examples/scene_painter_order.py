from manim import *


class ScenePainterOrder(Scene):
    def construct(self):
        def layer(color, offset):
            return Square(
                side_length=1.6,
                fill_color=color,
                fill_opacity=0.5,
                stroke_opacity=0.0,
            ).shift(offset)

        left_center = 1.8 * LEFT
        left_red = layer(RED, left_center + 0.3 * LEFT)
        left_green = layer(GREEN, left_center + 0.3 * RIGHT)
        left_blue = layer(BLUE, left_center + 0.3 * UP)

        right_center = 1.8 * RIGHT
        right_red = layer(RED, right_center + 0.3 * LEFT)
        right_green = layer(GREEN, right_center + 0.3 * RIGHT)
        right_blue = layer(BLUE, right_center + 0.3 * UP)

        # Start each cluster in the opposite order. These two operations must
        # restore the intended painter order while preserving caller order.
        self.add(left_blue, left_green, left_red)
        self.bring_to_back(left_red, left_green)

        self.add(right_red, right_green, right_blue)
        self.bring_to_front(right_green, right_red)

        self.play(right_red.animate.shift(ORIGIN))
