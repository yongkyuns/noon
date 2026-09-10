from manim import *

class OrdinaryPathSubcurves(Scene):
    def construct(self):
        source = Square(side_length=2, color=BLUE).shift(LEFT * 2)
        self.add(source)
        selected = source.get_subcurve(0.875, 0.375).shift(RIGHT * 4).set_stroke(YELLOW)
        assert len(selected.get_subpaths()) == 1
        assert not selected.is_closed()
        self.add(selected)
        self.wait(0.2)
