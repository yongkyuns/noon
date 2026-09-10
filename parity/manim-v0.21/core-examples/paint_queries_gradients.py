from manim import *


class PaintQueriesGradients(Scene):
    def construct(self):
        boxes = [Square(side_length=1).shift((index - 2) * 1.5 * RIGHT) for index in range(5)]
        family = VGroup(boxes[0], VGroup(*boxes))
        family.set_fill(opacity=0.7).set_stroke(width=2)
        self.add(family)
        family.set_color_by_gradient("#FF0000", "#00FF00", "#0000FF")
        boxes[0].set_color(boxes[0].get_color())
        assert abs(boxes[0].get_fill_opacity() - 0.7) < 1e-6
        assert abs(boxes[0].get_stroke_width() - 2) < 1e-6
        assert boxes[0].get_fill_color() == boxes[0].get_stroke_color()
        self.wait(0.2)
