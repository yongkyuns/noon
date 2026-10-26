"""One texture, two images: a plain reference and an animated shadow.

Paired with noon::example_scenes::shadow. Pixels are prepared once; the
ordinary Rust timeline owns motion and shadow interpolation during playback.
"""
from noon import (
    Shadow, Color, LEFT, RIGHT, UP, PI, ImageMobject, Pixels, Rectangle, Scene, smooth,
)


def diamond_pixels():
    pixels = []
    for y in range(9):
        row = []
        for x in range(9):
            distance = abs(x - 4) + abs(y - 4)
            alpha = 255 if distance <= 3 else 160 if distance == 4 else 48 if distance == 5 else 0
            # Hidden colored texels must not become a rectangular glow source.
            row.append([100 + 17 * x, 160 + 10 * y, 255, alpha] if alpha else [255, 0, 255, 0])
        pixels.append(row)
    return pixels


class ImageShadow(Scene):
    def construct(self):
        background = Rectangle(width=16, height=9, fill="#e0e8f0", fill_opacity=1, stroke=None)
        pixels = diamond_pixels()
        reference = ImageMobject(pixels, height=2, resampling_algorithm="bilinear").shift(2.2 * LEFT)
        image = ImageMobject(pixels, height=2, resampling_algorithm="bilinear").shift(2.2 * RIGHT)
        image.add_effect(Shadow(radius=Pixels(4), offset=Pixels((12,-10)), color=Color(0,0,0,0.65)), name="depth")
        self.add(background, reference, image)
        self.wait(0.75)

        self.play(
            image.animate.shift(0.6 * UP).rotate(PI / 12)
            .set_effect("depth", radius=Pixels(10), offset=Pixels((-12,12)), color=Color(0,0,1,0.8)),
            run_time=1.5, rate_func=smooth,
        )
        self.wait(0.75)

        # Return to the original pose while making the effect neutral.
        self.play(
            image.animate.shift(-0.6 * UP).rotate(-PI / 12)
            .set_effect("depth", radius=Pixels(4), offset=Pixels((12,-10)), color=Color(0,0,0,0)),
            run_time=1.5, rate_func=smooth,
        )
        self.wait(0.75)

        # Removing the neutral attachment leaves the same ordinary image.
        image.remove_effect("depth")
        self.wait(0.75)
