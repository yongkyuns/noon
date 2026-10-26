"""Fit, crop, tile, and move one texture with ordinary fill operations.

Paired with noon::example_scenes::image_paint. The image is an immutable source;
only the painted Rectangle is added to the scene. UV placement, fill opacity,
and normal object motion use one shared Rust target animation.
"""
from math import pi
from noon import Color, ImageMobject, ImagePaint, Rectangle, Scene, linear


class TexturedSurface(Scene):
    def construct(self):
        # Top-first rows; transparent colored texels must never leak hidden RGB.
        source = ImageMobject([
            [[255, 0, 0, 255], [0, 255, 0, 128], [0, 0, 255, 0], [255, 255, 0, 255]],
            [[0, 255, 255, 255], [255, 0, 255, 64], [255, 128, 0, 255], [32, 96, 224, 192]],
            [[128, 0, 255, 255], [255, 255, 255, 0], [0, 128, 255, 128], [255, 32, 96, 255]],
            [[16, 160, 80, 255], [96, 48, 192, 255], [224, 224, 224, 128], [0, 64, 192, 255]],
        ])
        panel = Rectangle(width=4, height=2, stroke=None)
        panel.set_fill(ImagePaint(source, sampling="nearest"), opacity=1)
        self.add(panel)
        self.wait(0.5)

        panel.set_fill(ImagePaint(source, fit="contain", sampling="nearest"))
        self.wait(0.5)
        panel.set_fill(ImagePaint(source, fit="stretch", sampling="nearest"))
        self.wait(0.5)
        panel.set_fill(ImagePaint(source, fit="stretch", address="repeat",
                                  scale=(0.5, 0.5), sampling="nearest"))
        self.wait(0.5)

        moving_pattern = ImagePaint(
            source, fit="stretch", address="repeat", sampling="nearest",
            translation=(0.2, -0.15), scale=(0.75, 0.6), rotation=pi / 4,
        )
        self.play(panel.animate.set_fill(moving_pattern, opacity=0.5).shift((0.5, 0.25, 0)),
                  run_time=1, rate_func=linear)
        self.wait(0.5)
        # Paint-kind changes are immediate; no implicit image/solid crossfade.
        panel.set_fill(Color(0.25, 0.5, 0.75), opacity=1)
        self.wait(0.5)
