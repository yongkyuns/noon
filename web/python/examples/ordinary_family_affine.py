"""Paired with the direct Rust/native/WASM family_affine example."""
from noon import Color, LEFT, PI, RIGHT, Scene, Square, VGroup


class OrdinaryFamilyAffine(Scene):
    def construct(self):
        a = Square(0.5, color=Color(0.2, 0.4, 1.0), fill_opacity=1, stroke_width=0).shift(LEFT)
        b = Square(0.5, color=Color(1.0, 0.8, 0.1), fill_opacity=1, stroke_width=0).shift(RIGHT)
        family = VGroup(VGroup(a, b), a)
        family.scale((2.0, 1.0))
        assert tuple(a.get_center()) == (-2.0, 0.0)
        assert tuple(b.get_center()) == (2.0, 0.0)
        self.add(family)
        self.wait(0.1)
        family.rotate(PI / 2)
        family.scale((0.5, 1.0))
        assert abs(a.get_center().y + 2.0) < 1e-6
        assert abs(b.get_center().y - 2.0) < 1e-6
        self.wait(0.1)
