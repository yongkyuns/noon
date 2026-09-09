"""Paired with the direct Rust/native/WASM family_grid example."""
from noon import Color, Rectangle, RIGHT, Scene, VGroup


class OrdinaryFamilyGrid(Scene):
    def construct(self):
        members = [Rectangle(width=width, height=height).shift(RIGHT).set_fill(Color(0.2, 0.6, 1), 0.7)
                   for width, height in ((2, 1), (1, 0.5), (0.5, 2), (1, 1))]
        family = VGroup(*members)
        family.arrange_in_grid(cols=2, buff=(0.5, 0.25))
        assert tuple(family.get_center()) == (1.0, 0.0)
        self.add(family)
        self.wait(0.1)
        family.arrange_in_grid(rows=1, buff=0.25)
        assert tuple(family.get_center()) == (1.0, 0.0)
        assert abs(family.width - 5.25) < 1e-6
        self.wait(0.1)
