"""Ordinary nested family effect targets, paired with Rust animated_program()."""
from noon import Blur, ImageMobject, Pixels, Rectangle, Scene, VGroup, linear


class ComposedEffectsAnimated(Scene):
    def construct(self):
        background = Rectangle(width=16, height=9, fill="#000000", fill_opacity=1, stroke=None)

        def source(x):
            pixels = [[[255, 255, 255, 0], [255, 255, 255, 128]],
                      [[255, 255, 255, 255], [255, 255, 255, 64]]]
            image = ImageMobject(pixels, height=1.2, resampling_algorithm="nearest").shift((x - 0.25, 0, 0))
            rectangle = Rectangle(width=1, height=0.8, fill="#ffffff", fill_opacity=0.5, stroke=None).shift((x + 0.25, 0, 0))
            return VGroup(image, rectangle).add_effect(Blur(radius=Pixels(2)), name="soften")

        reference = source(-2)
        inner = source(2)
        outer = VGroup(inner).set_glow(radius=Pixels(3.25), intensity=1.25, color="#ff0000")
        self.add(background, reference, outer)
        self.wait(0.5)
        self.play(
            reference.animate.set_effect("soften", radius=Pixels(4)),
            inner.animate.set_effect("soften", radius=Pixels(4)),
            outer.animate.set_glow(intensity=0.6),
            run_time=1, rate_func=linear,
        )
        self.wait(0.5)
        reference.remove_effect("soften")
        inner.remove_effect("soften")
        self.wait(0.5)
        outer.remove_glow()
        self.wait(0.5)
