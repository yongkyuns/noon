from manim import *


class OrdinaryZIndex(Scene):
    def construct(self):
        a = Square(side_length=2, z_index=1, color="#FF4466", fill_opacity=1, stroke_width=0).shift(.6 * LEFT + .3 * DOWN)
        b = Square(side_length=2, z_index=1, color="#4488FF", fill_opacity=1, stroke_width=0).shift(.6 * RIGHT + .3 * DOWN)
        c = Circle(radius=1, color="#44FF88", fill_opacity=1, stroke_width=0).shift(.5 * UP)
        nested = VGroup(a, b, z_index=1)
        family = VGroup(a, nested, z_index=-3)
        copied = family.copy()
        assert copied.z_index == -3
        assert copied[0].z_index == 1
        self.add(family, c)
        c.set_z_index(2.25)
        a.set_z_index(3.5)
        family.set_z_index(.5, family=False)
        assert a.z_index == 3.5
        assert b.z_index == 1
        self.wait(.2)
