from noon import *


class OrdinaryCanonicalCurveLayout(Scene):
    def construct(self):
        def assert_empty_extrema_unavailable(value):
            for coordinate in (value.get_x, value.get_y):
                try:
                    coordinate()
                except ValueError:
                    continue
                raise AssertionError("an empty Mobject has no coordinate extrema")

        empty = Mobject().shift(3 * RIGHT - 2 * UP)
        assert_empty_extrema_unavailable(empty)
        self.add(empty)
        circle = Circle(radius=1, color=WHITE).stretch_to_fit_width(4).stretch_to_fit_height(1.5)
        ellipse = Ellipse(width=4, height=1.5, color="#58c4dd")
        for shape, x in ((circle, -2.4), (ellipse, 2.4)):
            shape.rotate(PI / 6).shift(RIGHT * x)
            marker = Square(side_length=0.12, fill_color=YELLOW, fill_opacity=1, stroke_width=0)
            marker.move_to(shape.get_right())
            self.add(shape, marker)
        self.wait(0.2)
        assert_empty_extrema_unavailable(empty)
