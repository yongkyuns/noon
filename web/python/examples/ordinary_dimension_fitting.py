"""Paired with the native/direct Rust-WASM dimension_fitting example."""
from noon import *


class DimensionFitting(Scene):
    def construct(self):
        a = Rectangle(width=2.0, height=1.0).set_fill(Color(0.2, 0.4, 1.0), opacity=1.0).set_stroke(width=0)
        b = Square(side_length=1.0).set_fill(Color(1.0, 0.8, 0.1), opacity=1.0).set_stroke(width=0)
        assert a.rescale_to_fit(float("nan"), 2, about_point=object()) is a
        a.scale_to_fit_height(2.0)
        a.match_height(b, stretch=True)
        a.shift(LEFT * 2)
        b.shift(RIGHT * 2)
        family = VGroup(a, b, a)
        family.width = 4.0
        self.add(family)
        live = self.live_execution()
        assert family.rescale_to_fit(float("inf"), 2, stretch=True) is family
        assert a.match_dim_size(b, 2, about_edge=object()) is a
        a.match_width(b, stretch=True)
        family.height = 2.0
        assert abs(a.width - b.width) < 1e-6
        assert abs(family.height - 2.0) < 1e-6
        live.wait(0.2)
