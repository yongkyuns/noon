"""A moving line pattern, paired with noon::example_scenes::hatch_paint."""
from noon import Color, HatchPaint, Rectangle, Scene, linear


class HatchedSurface(Scene):
    def construct(self):
        first = HatchPaint(Color(1, 0, 0, 0.5), Color(0, 0, 1),
                           spacing=0.6, width=0.09, angle=0.2,
                           phase=(-0.173, 0.231), cross=True)
        second = HatchPaint(Color(0, 1, 0), Color(0, 0, 1, 0),
                            spacing=0.35, width=0.12, angle=0.85,
                            phase=(0.531, -0.719), cross=True)
        panel = Rectangle(width=4, height=2, stroke=None).set_fill(first, opacity=1)
        self.add(panel)
        self.wait(0.5)
        self.play(panel.animate.set_fill(second, opacity=0.5), run_time=1, rate_func=linear)
        self.wait(0.25)
        self.play(panel.animate.set_fill(Color(1, 0, 0), opacity=1), run_time=1, rate_func=linear)
        self.wait(0.25)
        self.play(panel.animate.set_fill(first), run_time=1, rate_func=linear)
        self.wait(0.25)
