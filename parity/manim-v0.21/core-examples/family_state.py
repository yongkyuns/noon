from manim import *


class OrdinaryFamilyState(Scene):
    def construct(self):
        a = Square(side_length=0.6, color="#FF0000", fill_opacity=1, stroke_width=0).shift(LEFT * 2)
        b = Square(side_length=0.6, color="#0000FF", fill_opacity=1, stroke_width=0)
        nested = VGroup(a, b)
        family = VGroup(a, nested)
        family.save_state()
        replacement = family.copy().shift(RIGHT).scale(1.5)
        family.become(replacement, match_width=True)
        assert family[0] is a and family[1][0] is a
        self.add(family)
        family.generate_target()
        family.target.shift(RIGHT + UP)
        self.play(MoveToTarget(family, run_time=0.4, rate_func=linear))
        family.restore()
        assert abs(a.get_center()[0] + 2) < 1e-6
        assert abs(b.get_center()[0]) < 1e-6
        self.wait(0.2)
