from noon import *


class StyleOperations(Scene):
    async def construct(self):
        a = Square(side_length=1).shift(2 * LEFT)
        b = Circle(radius=0.5)
        c = Square(side_length=1).shift(2 * RIGHT)
        family = VGroup(a, VGroup(a, b))
        palette = family.copy().set_style(
            fill_color="#0000FF", fill_opacity=0.7,
            stroke_color="#FF0000", stroke_width=6,
        )
        family.match_style(palette)
        family.set_sheen_direction([1, 0])
        assert list(a.get_sheen_direction()) == [1, 0]
        line = Line().set_cap_style(CapStyleType.ROUND)
        assert line.cap_style is CapStyleType.ROUND
        self.add(family, c)
        c.match_style(a)
        c.set_background_stroke(color="#0000FF", width=2, opacity=0.25)
        assert c.get_stroke_color(background=True) == Color(0, 0, 1)
        assert abs(c.get_stroke_width(background=True) - 2) < 1e-6
        assert abs(c.get_stroke_opacity(background=True) - 0.25) < 1e-6
        c.set_background_stroke(width=0)
        family.set_style(fill_color="#FF8000")
        family.set_fill().set_stroke()
        await self.wait(0.1)
        late_line = Line().set_cap_style(CapStyleType.SQUARE)
        assert late_line.cap_style is CapStyleType.SQUARE
        self.add(late_line)
        assert late_line.cap_style is CapStyleType.SQUARE
        self.remove(late_line)
        assert late_line.cap_style is CapStyleType.SQUARE
        c.set_background_stroke(color="#FF0000", width=4, opacity=0.75)
        assert c.get_stroke_color(background=True) == Color(1, 0, 0)
        assert abs(c.get_stroke_width(background=True) - 4) < 1e-6
        assert abs(c.get_stroke_opacity(background=True) - 0.75) < 1e-6
        c.set_background_stroke(width=0)
        family.rotate_sheen_direction(PI / 2)
        assert abs(a.get_sheen_direction()[1] - 1) < 1e-9
        c.set_sheen_direction([0, -1])
        assert list(c.get_sheen_direction()) == [0, -1]
        await self.wait(0.1)
