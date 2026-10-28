"""An elliptical local paint, paired with noon::example_scenes::radial_gradient."""
from noon import Color, RadialGradient, Rectangle, Scene, linear


class RadialSurface(Scene):
    def construct(self):
        first = RadialGradient([(0, Color(1, 0, 0, 0.5)), (1, Color(0, 0, 1))],
                               center=(0.25, 0.5), radii=(0.5, 0.75))
        second = RadialGradient([(0, Color(0, 1, 0)), (1, Color(0, 0, 1, 0))],
                                center=(0.65, 0.35), radii=(0.75, 0.4))
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
