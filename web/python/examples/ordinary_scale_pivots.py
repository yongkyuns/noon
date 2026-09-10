from noon import *


class OrdinaryScalePivots(Scene):
    def construct(self):
        line = Line(3 * LEFT + UP, LEFT + 1.5 * UP).set_stroke("#4488FF")
        line.scale(1.5)
        other = Line(RIGHT + UP, 3 * RIGHT + 1.5 * UP).set_stroke("#44FF88")
        other.scale(1.5, about_edge=RIGHT)
        a = Square(side_length=.6, color="#FF4466", fill_opacity=1, stroke_width=0).shift(LEFT + DOWN)
        b = Square(side_length=.6, color="#FFCC44", fill_opacity=1, stroke_width=0).shift(RIGHT + DOWN)
        family = VGroup(a, VGroup(a, b))
        family.scale(1.25, about_point=ORIGIN)
        self.add(line, other, family)
        line.scale(.8)
        other.scale(.8, about_point=2 * RIGHT + UP)
        family.scale(1.2, about_edge=LEFT)
        line.scale_to_fit_width(2, about_edge=LEFT)
        other.match_height(line, about_point=2 * RIGHT + UP)
        family.scale_to_fit_width(4, about_edge=RIGHT)
        self.wait(.2)
