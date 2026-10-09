"""Paired ordinary/glow sources for the production Python render-worker path.

The two circles have identical paint and differ only in the attached glow.
The left half therefore provides an in-frame no-effect negative control.
"""
from noon import BLUE, WHITE, Circle, LEFT, RIGHT, Pixels, Scene


class GlowSceneWorkerPixels(Scene):
    def construct(self):
        ordinary = Circle(radius=0.4, fill=WHITE, fill_opacity=1, stroke=None)
        ordinary.shift(LEFT * 2)
        glowing = Circle(radius=0.4, fill=WHITE, fill_opacity=1, stroke=None)
        glowing.shift(RIGHT * 2)
        glowing.set_glow(color=BLUE, radius=Pixels(6.5), intensity=1.4)
        self.add(ordinary, glowing)
        self.wait(0.25)
