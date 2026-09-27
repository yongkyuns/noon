"""Animate a recognizable array-backed raster without network or image-library dependencies."""
from noon import *


def landscape(width=48, height=32):
    pixels = []
    for y in range(height):
        row = []
        for x in range(width):
            color = [35 + y * 2, 90 + y * 2, 165 + y, 255]
            if (x - 36) ** 2 + (y - 8) ** 2 < 16:
                color = [255, 218, 112, 255]
            if y > 10 + abs(x - 17) * 0.7:
                color = [48, 83, 107, 255]
            if y > 19 + abs(x - 37) * 0.35:
                color = [39, 125, 105, 255]
            row.append(color)
        pixels.append(row)
    return pixels


class RasterImages(Scene):
    def construct(self):
        title = Text("Pixels become part of the scene", font_size=34).shift(2.9 * UP)
        caption = Text("An array-backed image: reveal, rotate, and fade", font_size=22).shift(2.6 * DOWN)
        pixels = landscape()
        image = ImageMobject(pixels, height=2.7, resampling_algorithm="nearest").shift(2.2 * LEFT)
        second = ImageMobject(pixels, height=1.7, resampling_algorithm="nearest").shift(2.4 * RIGHT)
        self.play(FadeIn(title), FadeIn(caption), run_time=0.7)
        self.play(FadeIn(image), run_time=1.2)
        self.play(FadeIn(second), run_time=1.0)
        self.wait(0.5)
        resting = image.copy()
        tilted = image.copy().rotate(PI / 12).set_opacity(0.65)
        self.play(Transform(image, tilted), run_time=1.5, rate_func=smooth)
        self.play(Transform(image, resting), run_time=1.5, rate_func=smooth)
        self.wait(1.2)
