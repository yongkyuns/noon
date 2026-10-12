"""Persistent glow on a circle, stroked path, and MathTypst; paired with Rust."""
from noon import (
    BLUE, RED, WHITE, Circle, Create, DOWN, LEFT, MathTypst, Pixels, RIGHT, Scene,
    UP, VMobject, Write, linear,
)


class GlowScenePlayback(Scene):
    def construct(self):
        dot = Circle(radius=0.4, fill=WHITE, fill_opacity=1, stroke=None)
        dot.set_glow(color=RED, radius=Pixels(3.25), intensity=0.4000000000000123)
        self.add(dot)
        original = dot.get_effect("glow")
        self.play(
            dot.animate.shift(RIGHT * 2).set_glow(
                color=BLUE, radius=Pixels(6.5), intensity=1.4),
            run_time=1.0, rate_func=linear,
        )
        assert dot.get_center() == (2.0, 0.0)
        assert original.authored_definition.intensity == 1.4
        assert original.authored_definition.radius == Pixels(6.5)
        path = VMobject(stroke_color=BLUE, stroke_width=5, fill_opacity=0)
        path.start_new_path(2.5 * LEFT + 0.7 * DOWN)
        path.add_cubic_bezier_curve_to(1.5 * LEFT + 1.2 * UP, 1.5 * RIGHT + 1.2 * DOWN, 2.5 * RIGHT + 0.7 * UP)
        formula = MathTypst("integral_0^1 x^2 dif x = frac(1, 3)", font_size=34).shift(0.2 * DOWN)
        path.set_glow(color=BLUE, radius=Pixels(7), intensity=1.2)
        formula.set_glow(color=RED, radius=Pixels(6), intensity=0.9)
        path_glow = path.get_effect("glow")
        formula_glow = formula.get_effect("glow")
        self.play(
            Create(path), Write(formula),
            path.animate.set_glow(intensity=1.8, radius=Pixels(10)),
            formula.animate.set_glow(intensity=1.4, color=BLUE),
            run_time=1.5, rate_func=linear,
        )
        assert path_glow.authored_definition.intensity == 1.8
        assert path_glow.authored_definition.radius == Pixels(10)
        assert formula_glow.authored_definition.intensity == 1.4
        assert formula_glow.authored_definition.radius == Pixels(6)
        self.play(dot.animate.set_glow(intensity=0.0), run_time=0.5, rate_func=linear)
        assert original.authored_definition.intensity == 0.0
        dot.remove_glow()
        try:
            original.authored_definition
        except ReferenceError:
            pass
        else:
            raise AssertionError("removed attachment handle must stay retired")
        self.wait(0.25)
        assert dot.get_center() == (2.0, 0.0)
