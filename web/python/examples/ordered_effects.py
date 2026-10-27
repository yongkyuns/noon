"""Ordered appearance with two independently animated blurs.

Paired with noon::example_scenes::ordered_effects. The left object supplies
ordinary source coverage; the right applies each attachment in authored order.
"""
from noon import Blur, Color, LEFT, Pixels, Rectangle, RIGHT, Scene, Shadow, UP, linear


class OrderedEffects(Scene):
    def construct(self):
        background = Rectangle(width=16, height=9, fill="#000000", fill_opacity=1, stroke=None)
        reference = Rectangle(width=1.2, height=0.8, fill="#ffffff", fill_opacity=1, stroke=None).shift(2 * LEFT)
        source = Rectangle(width=1.2, height=0.8, fill="#ffffff", fill_opacity=1, stroke=None).shift(2 * RIGHT)
        source.set_object_opacity(0.5)
        source.add_effect(Blur(radius=Pixels(2)), name="soften")
        source.set_glow(radius=Pixels(3.25), intensity=1.25, color="#ff0000")
        source.add_effect(Shadow(radius=Pixels(2), offset=Pixels((8, -4)), color=Color(0, 0, 1, 0.7)), name="depth")
        source.add_effect(Blur(radius=Pixels(1)), name="finish")
        self.add(background, reference, source)
        self.wait(0.5)
        self.play(
            source.animate.shift(0.4 * UP).set_effect("soften", radius=Pixels(4)),
            source.animate.set_effect("finish", radius=Pixels(3)),
            reference.animate.shift(0.4 * UP),
            run_time=1, rate_func=linear,
        )
        self.wait(0.5)
        self.play(
            source.animate.shift(-0.4 * UP)
            .set_effect("soften", radius=Pixels(0))
            .set_effect("finish", radius=Pixels(0))
            .set_effect("glow", intensity=0)
            .set_effect("depth", color=Color(0, 0, 1, 0)),
            reference.animate.shift(-0.4 * UP),
            run_time=1, rate_func=linear,
        )
        self.wait(0.5)
        for name in ("soften", "glow", "depth", "finish"):
            source.remove_effect(name)
        self.wait(0.5)
