from noon import *


class OrdinaryCanonicalCurveLayout(Scene):
    def construct(self):
        circle = Circle(radius=1, color=WHITE).stretch_to_fit_width(4).stretch_to_fit_height(1.5)
        ellipse = Ellipse(width=4, height=1.5, color="#58c4dd")
        for shape, x in ((circle, -2.4), (ellipse, 2.4)):
            shape.rotate(PI / 6).shift(RIGHT * x)
            marker = Square(side_length=0.12, fill_color=YELLOW, fill_opacity=1, stroke_width=0)
            marker.move_to(shape.get_right())
            self.add(shape, marker)
        self.wait(0.2)
