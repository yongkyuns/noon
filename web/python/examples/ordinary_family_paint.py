"""Paired with the direct Rust/native/WASM family_paint example."""
from noon import Color, LEFT, RIGHT, Scene, Square, VGroup


class OrdinaryFamilyPaint(Scene):
    async def construct(self):
        a = Square(0.6).shift(LEFT)
        b = Square(0.6).shift(RIGHT)
        family = VGroup(VGroup(a, b), a)
        family.set_fill(Color(1, 0, 0), 0.8)
        family.set_stroke(Color(0, 0, 1), width=4, opacity=1)
        a.fade()
        assert a.get_fill_opacity() == 0.4
        self.add(family)
        await self.wait(0.1)
        family.set_color(Color(0.2, 0.6, 1.0))
        family.set_opacity(0.5)
        for member in (a, b):
            assert member.get_fill_opacity() == 0.5
            assert member.get_stroke_opacity() == 0.5
        a.fade(0.5, family=False)
        family.fade(0.5)
        assert a.get_fill_opacity() == a.get_stroke_opacity() == 0.0625
        assert b.get_fill_opacity() == b.get_stroke_opacity() == 0.25
        family.fade_to(Color(1, 0, 0), 0.5)
        for member, expected in ((a, (0.8, 0.15, 0.25)), (b, (0.6, 0.3, 0.5))):
            for paint in (member.get_fill_color(), member.get_stroke_color()):
                assert abs(paint.red - expected[0]) < 1e-6
                assert abs(paint.green - expected[1]) < 1e-6
                assert abs(paint.blue - expected[2]) < 1e-6
        await self.wait(0.1)
