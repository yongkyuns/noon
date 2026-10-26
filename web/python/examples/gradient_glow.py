"""A painted surface retains its glow while its paint changes, then removes it.

Paired with noon::example_scenes::linear_gradient::glow_program.
"""
from noon import Color, Glow, LinearGradient, Pixels, Rectangle, Scene, linear


class PaintedLight(Scene):
    def construct(self):
        red, green, blue = Color(1, 0, 0), Color(0, 1, 0), Color(0, 0, 1)
        horizontal = LinearGradient([(0, red), (1, blue)])
        vertical = LinearGradient([(0, green), (1, blue)], start=(0.5, 0), end=(0.5, 1))
        panel = Rectangle(width=4, height=2, stroke=None)
        panel.set_fill(horizontal, opacity=1)
        panel.add_effect(Glow(color=Color(1, 1, 1), radius=Pixels(6), intensity=0.4), name="glow")
        self.add(panel)
        self.wait(0.5)
        self.play(
            panel.animate.set_fill(vertical).set_effect("glow", radius=Pixels(10), intensity=1.4),
            run_time=1, rate_func=linear,
        )
        self.wait(0.25)
        self.play(panel.animate.set_fill(red), run_time=1, rate_func=linear)
        self.wait(0.25)
        self.play(panel.animate.set_fill(horizontal), run_time=1, rate_func=linear)
        self.wait(0.25)
        panel.remove_effect("glow")
        self.wait(0.25)
