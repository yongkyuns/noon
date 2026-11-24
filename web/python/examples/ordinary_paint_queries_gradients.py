from noon import *


class PaintQueriesGradients(Scene):
    def construct(self):
        boxes = [Square(side_length=1).shift((index - 2) * 1.5 * RIGHT) for index in range(5)]
        family = VGroup(VGroup(*boxes), boxes[0])
        family.set_fill(opacity=0.7).set_stroke(width=2)
        self.add(family)
        family.set_color_by_gradient("#FF0000", "#00FF00", "#0000FF")
        boxes[0].set_color(boxes[0].get_color())
        original = boxes[0].get_color()
        boxes[0].match_color(boxes[-1])
        assert boxes[0].get_color() == boxes[-1].get_color()
        boxes[0].set_color(original)
        assert abs(boxes[0].get_fill_opacity() - 0.7) < 1e-6
        assert abs(boxes[0].get_stroke_width() - 2) < 1e-6
        assert boxes[0].get_fill_color() == boxes[0].get_stroke_color()
        self.wait(0.1)

        family.set_colors_by_radial_gradient(
            center=ORIGIN, radius=2, inner_color="#FF0000", outer_color="#0000FF",
        )
        assert boxes[2].get_fill_color().red == 1
        assert boxes[-1].get_fill_color().blue == 1
        boxes[-1].set_colors_by_radial_gradient(
            center=boxes[-1].get_center(), radius=1,
            inner_color="#00FF00", outer_color="#0000FF",
        )
        assert boxes[-1].get_fill_color().green == 1
        assert abs(boxes[0].get_fill_opacity() - 0.7) < 1e-6
        family.set_color_by_gradient("#FF0000", "#00FF00", "#0000FF")
        self.wait(0.1)
