"""Paired with the direct Rust/native/WASM family_paint example."""
from noon import Color, LEFT, RIGHT, Scene, Square, VGroup


class OrdinaryFamilyPaint(Scene):
    def construct(self):
        a = Square(0.6).shift(LEFT)
        b = Square(0.6).shift(RIGHT)
        family = VGroup(VGroup(a, b), a)
        family.set_fill(Color(1, 0, 0), 0.8)
        family.set_stroke(Color(0, 0, 1), width=4, opacity=1)
        self.add(family)
        self.wait(0.1)
        family.set_color(Color(0.2, 0.6, 1.0))
        family.set_opacity(0.5)
        for member in (a, b):
            assert member.get_fill_opacity() == 0.5
            assert member.get_stroke_opacity() == 0.5
        self.wait(0.1)
