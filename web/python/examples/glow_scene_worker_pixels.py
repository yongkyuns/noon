"""Paired ordinary/glow sources for the production Python render-worker path.

The two circles have identical paint and differ only in the attached glow.
The left half therefore provides an in-frame no-effect negative control.
"""
from noon import BLUE, WHITE, Circle, LEFT, RIGHT, Pixels, Scene, linear


class GlowSceneWorkerPixels(Scene):
    def construct(self):
        ordinary = Circle(radius=0.4, fill=WHITE, fill_opacity=1, stroke=None)
        ordinary.shift(LEFT * 2)
        glowing = Circle(radius=0.4, fill=WHITE, fill_opacity=1, stroke=None)
        glowing.shift(RIGHT * 2)
        self.add(ordinary, glowing)
        # The glow is absent until activation; the shared Rust transaction
        # installs it neutral and animates intensity through the normal driver.
        self.play(glowing.animate.set_glow(
            color=BLUE, radius=Pixels(6.5), intensity=1.4),
            run_time=0.25, rate_func=linear)
