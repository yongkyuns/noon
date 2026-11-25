"""Cloud and paper paints through native NoisePaint; no shader or frame callback.

Paired with noon::example_scenes::fractal_noise.
"""
from noon import Color, NoisePaint, Rectangle, Scene, linear


class FractalSurface(Scene):
    def construct(self):
        low, high = Color(0.05, 0.10, 0.25), Color(0.50, 0.80, 1.0)
        clouds = NoisePaint.fractal(low, high, seed=7)
        sky = Rectangle(width=3.6, height=2.4, stroke=None).shift((-2.1, 0, 0))
        sky.set_fill(clouds, opacity=1)

        paper_low, paper_high = Color(0.86, 0.80, 0.68), Color(1, 0.97, 0.90)
        grain = NoisePaint.fractal(
            paper_low, paper_high, seed=7, frequency=(18, 18),
            stops=[(0, paper_low), (0.45, Color(0.96, 0.91, 0.81)), (1, paper_high)],
        )
        paper = Rectangle(width=3.6, height=2.4, stroke=None).shift((2.1, 0, 0))
        paper.set_fill(grain, opacity=1)
        swatch = Rectangle(width=1.4, height=0.45, stroke=None).shift((-2.1, -2.3, 0))
        swatch.set_fill(clouds, opacity=1)
        self.add(sky, paper, swatch)
        self.wait(0.5)
        self.play(
            sky.animate.shift((0, 0.4, 0)).set_fill(
                NoisePaint.fractal(low, high, seed=7, phase=(1.5, 0)),
            ),
            run_time=3, rate_func=linear,
        )
        self.wait(0.5)
