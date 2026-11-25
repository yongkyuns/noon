"""Reveal, burn away, and restore one source through native visibility semantics.

Paired with noon::example_scenes::radial_reveal. No shader or frame callback is
needed; the in/out gestures reuse one canonical mask and ordinary authored time.
"""
from noon import Color, RadialReveal, Rectangle, Scene, linear


class RadialOpening(Scene):
    def construct(self):
        panel = Rectangle(width=4, height=2, fill=Color(0.2, 0.4, 0.9),
                          fill_opacity=1, stroke=None)
        self.play(RadialReveal(panel), run_time=1, rate_func=linear)
        self.wait(0.25)
        self.play(RadialReveal(panel, direction="out", edge="burn", seed=7,
                               remover=False), run_time=1, rate_func=linear)
        self.wait(0.25)
        self.play(RadialReveal(panel), run_time=1, rate_func=linear)
        self.wait(0.25)
