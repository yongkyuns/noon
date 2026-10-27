"""Seeded procedural appearance paired with noon::example_scenes::noise_paint."""
from noon import Color, NoisePaint, Rectangle, Scene, linear


class NoiseSurface(Scene):
    def construct(self):
        first = NoisePaint(Color(1, 0, 0, 0.5), Color(0, 0, 1), seed=7,
                           frequency=(8, 8), phase=(-0.173, 0.231))
        second = NoisePaint(Color(0, 1, 0), Color(0, 0, 1, 0), seed=7,
                            frequency=(32, 12), phase=(0.531, -0.719))
        panel = Rectangle(width=4, height=2, stroke=None)
        panel.set_fill(first, opacity=1)
        self.add(panel)
        self.wait(0.5)
        self.play(panel.animate.set_fill(second, opacity=0.5), run_time=1, rate_func=linear)
        self.wait(0.25)
        self.play(panel.animate.set_fill(Color(1, 0, 0), opacity=1), run_time=1, rate_func=linear)
        self.wait(0.25)
        self.play(panel.animate.set_fill(first), run_time=1, rate_func=linear)
        self.wait(0.25)
