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
        self.add(family, c)
        c.match_style(a)
        family.set_style(fill_color="#FF8000")
        family.set_fill().set_stroke()
        await self.wait(0.1)
        family.rotate_sheen_direction(PI / 2)
        assert abs(a.get_sheen_direction()[1] - 1) < 1e-9
        c.set_sheen_direction([0, -1])
        assert list(c.get_sheen_direction()) == [0, -1]
        await self.wait(0.1)
