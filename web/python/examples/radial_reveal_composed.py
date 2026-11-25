"""One native mask over image alpha and one over an overlapping composition.

Paired with noon::example_scenes::radial_reveal_composed. Movement and opacity
share ordinary play with the visibility gestures; no shader or callback is used.
"""
from noon import *


def image_pixels():
    pixels = [
        [[51, 153, 255, 128], [51, 153, 255, 255], [51, 153, 255, 255], [51, 153, 255, 128]],
        [[51, 153, 255, 255], [255, 0, 255, 0], [255, 0, 255, 0], [51, 153, 255, 255]],
        [[51, 153, 255, 255], [255, 0, 255, 0], [255, 0, 255, 0], [51, 153, 255, 255]],
        [[51, 153, 255, 128], [51, 153, 255, 255], [51, 153, 255, 255], [51, 153, 255, 128]],
    ]
    return pixels


class ComposedRadialOpening(Scene):
    def construct(self):
        image = ImageMobject(image_pixels(), height=1.6, resampling_algorithm="nearest").shift((-2, 0, 0))
        left = Rectangle(width=2, height=1.4, fill=Color(0.2, 0.4, 0.9),
                         fill_opacity=0.55, stroke=None).shift((1.7, 0, 0))
        right = Rectangle(width=2, height=1.4, fill=Color(0.95, 0.4, 0.1),
                          fill_opacity=0.55, stroke=None).shift((2.3, 0, 0))
        group = VGroup(left, right)
        self.play(RadialReveal(image), RadialReveal(group, edge="burn", seed=7),
                  run_time=1, rate_func=linear)
        self.wait(0.25)
        self.play(
            RadialReveal(image, direction="out", edge="burn", seed=7, remover=False),
            RadialReveal(group, direction="out", edge="clean", remover=False),
            image.animate.shift(0.5 * UP).set_object_opacity(0.6),
            group.animate.shift(0.5 * UP).set_object_opacity(0.6),
            run_time=1, rate_func=linear,
        )
        self.wait(0.25)
