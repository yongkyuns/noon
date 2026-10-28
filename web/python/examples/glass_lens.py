"""Move a glass card across a texture, then neutralize and dissolve the lens.

Paired with noon::example_scenes::glass_lens. The shared Rust timeline owns
motion, radius interpolation and seeded visibility; pixels are prepared once.
"""
from noon import (
    BackdropBlur, Dissolve, ImageMobject, Pixels, Rectangle, Scene, Text, VGroup,
    linear, smooth,
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



class GlassLens(Scene):
    def construct(self):
        self.camera.frame.scale(1.125)
        background = Rectangle(width=16, height=9, fill="#08101f", fill_opacity=1, stroke=None)
        pixels = [
            [[28, 82, 148, 255] if (x // 3 + y // 3) % 2 == 0 else [240, 166, 70, 255]
             for x in range(48)] for y in range(24)
        ]
        texture = ImageMobject(pixels, height=6, resampling_algorithm="nearest")
        panel = Rectangle(width=3, height=2, fill="#ffffff", fill_opacity=0.12, stroke=None).shift((-2, 0, 0))
        emblem = ImageMobject(diamond_pixels(), height=0.6, resampling_algorithm="bilinear").shift((-2.95, 0.55, 0))
        label = Text("GLASS", font_size=24).shift((-2, -0.35, 0))
        card = VGroup(panel, emblem, label).set_opacity(0.85)
        card.add_effect(BackdropBlur(radius=Pixels(6)), name="glass")
        self.add(background, texture, card)
        self.wait(0.5)
        self.play(card.animate.shift((4, 0, 0)).set_effect("glass", radius=Pixels(18)), run_time=1.5, rate_func=smooth)
        self.wait(0.5)
        self.play(card.animate.shift((-4, 0, 0)).set_effect("glass", radius=Pixels(0)), run_time=1.5, rate_func=smooth)
        self.wait(0.5)
        card.remove_effect("glass")
        self.wait(0.5)
        card.add_effect(BackdropBlur(radius=Pixels(12)), name="glass")
        self.wait(0.5)
        self.play(Dissolve(card, direction="out", seed=7, softness=0.2, remover=False), run_time=1, rate_func=linear)
        self.wait(0.5)
        self.play(Dissolve(card, direction="in", seed=7, softness=0.2, remover=False), run_time=1, rate_func=linear)
        self.wait(0.5)
