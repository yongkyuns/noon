from manim import *


class StyleOperations(Scene):
    def construct(self):
        a = Square(side_length=1).shift(2 * LEFT)
        b = Circle(radius=0.5)
        c = Square(side_length=1).shift(2 * RIGHT)
        family = VGroup(a, VGroup(a, b))
        palette = family.copy().set_style(
            fill_color="#0000FF", fill_opacity=0.7,
            stroke_color="#FF0000", stroke_width=6,
        )
        family.match_style(palette)
        self.add(family, c)
        c.match_style(a)
        family.set_style(fill_color="#FF8000")
        family.set_fill().set_stroke()
        self.wait(0.2)
