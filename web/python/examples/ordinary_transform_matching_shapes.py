from noon import *


def triangle():
    return VMobject().set_points_as_corners(
        [(-1, -1, 0), (1, -0.5, 0), (-0.25, 1, 0), (-1, -1, 0)]
    )


def kite():
    return VMobject().set_points_as_corners(
        [(0, -1, 0), (1.5, 0, 0), (0, 1, 0), (-0.5, 0, 0), (0, -1, 0)]
    )


def rotated_triangle():
    return VMobject().set_points_as_corners(
        [(1, -1, 0), (0.5, 1, 0), (-1, -0.25, 0), (1, -1, 0)]
    )


class OrdinaryTransformMatchingShapes(Scene):
    """Match duplicate triangle keys while fading one unmatched kite and triangle."""

    def construct(self):
        source_first = triangle().set_fill(BLUE, opacity=0.9).set_stroke(opacity=0).shift(LEFT * 4 + UP * 1.5)
        source_second = triangle().set_fill(GREEN, opacity=0.9).set_stroke(opacity=0).shift(LEFT + UP * 1.5)
        source_leftover = kite().set_fill(RED, opacity=0.9).set_stroke(opacity=0).shift(LEFT * 4 + DOWN * 1.5)
        source = VGroup(source_first, source_second, source_leftover)
        self.add(source)

        target_first = triangle().set_fill(YELLOW, opacity=0.9).set_stroke(opacity=0).shift(LEFT * 4 + DOWN * 1.5)
        target_second = triangle().set_fill(PINK, opacity=0.9).set_stroke(opacity=0).shift(LEFT + DOWN * 1.5)
        target_padded = triangle().set_fill(BLUE, opacity=0.9).set_stroke(opacity=0).shift(RIGHT * 2 + DOWN * 1.5)
        target_leftover = rotated_triangle().set_fill(WHITE, opacity=0.9).set_stroke(opacity=0).shift(RIGHT * 4 + UP * 1.5)
        target = VGroup(target_first, target_second, target_padded, target_leftover)

        self.play(TransformMatchingShapes(source, target, run_time=1.0, rate_func=linear))
        self.play(Indicate(target, run_time=1.0))
