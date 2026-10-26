"""A painted surface, paired with noon::example_scenes::linear_gradient.

Paints share the ordinary fill/target animation path. Stops are ordered and
coordinates use the object's local box. Omitted opacity preserves its multiplier.
"""
from noon import Color, LinearGradient, Rectangle, Scene, linear


class PaintedSurface(Scene):
    def construct(self):
        red, green, blue = Color(1, 0, 0), Color(0, 1, 0), Color(0, 0, 1)
        horizontal = LinearGradient([(0, red), (1, blue)])
        vertical = LinearGradient([(0, green), (1, blue)], start=(0.5, 0), end=(0.5, 1))
        panel = Rectangle(width=4, height=2, stroke=None)
        panel.set_fill(horizontal, opacity=1)
        self.add(panel)
        self.wait(0.5)
        self.play(panel.animate.set_fill(vertical, opacity=0.5), run_time=1, rate_func=linear)
        self.wait(0.25)
        self.play(panel.animate.set_fill(red, opacity=1), run_time=1, rate_func=linear)
        self.wait(0.25)
        self.play(panel.animate.set_fill(horizontal), run_time=1, rate_func=linear)
        self.wait(0.25)
