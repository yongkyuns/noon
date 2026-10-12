"""Paired with noon::example_scenes::raster_image, including late creation."""
from noon import *

PIXELS = [
    [[255, 0, 0, 255], [0, 255, 0, 128]],
    [[0, 0, 255, 0], [255, 255, 255, 255]],
]


class RasterImage(Scene):
    def construct(self):
        background = Square(8).set_fill("#14283c", opacity=1).set_stroke(width=0)
        self.add(background)
        image = ImageMobject(PIXELS, height=2, resampling_algorithm="nearest").shift(2 * LEFT)
        self.play(FadeIn(image), run_time=1, rate_func=linear)
        # An independent semantic identity; immutable pixels deduplicate in Rust.
        second = ImageMobject(PIXELS, height=1, resampling_algorithm="nearest").shift(2 * RIGHT)
        self.add(second)
        target = image.copy().move_to(ORIGIN).rotate(PI / 4).scale(0.75).set_opacity(0.6)
        self.play(Transform(image, target), run_time=1, rate_func=linear)
        self.play(FadeOut(image), run_time=1, rate_func=linear)
        self.wait(1)
