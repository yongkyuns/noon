"""Draw a function and an explicitly identified sampled series on common axes."""
from math import sin
from noon import *


class FunctionsAndSamples(Scene):
    def construct(self):
        title = Text("A function and sampled observations", font_size=32).shift(3 * UP)
        axes = Axes([0, 10, 2], [-1.5, 1.5, 0.5], x_length=10, y_length=4)
        model = axes.plot(lambda t: sin(0.8 * t), [0, 10, 0.05], color=BLUE)
        observations = [(0, 0.10), (2, 0.95), (4, -0.10), (6, -0.90), (8, 0.20), (10, 1.00)]
        samples = axes.plot_samples(observations, color=YELLOW)
        model_label = Text("Function", font_size=22, color=BLUE).move_to((-2, -2.7, 0))
        sample_label = Text("Illustrative samples", font_size=22, color=YELLOW).move_to((2, -2.7, 0))
        self.play(FadeIn(title), run_time=0.7)
        self.play(Create(axes), run_time=1.2, rate_func=smooth)
        self.play(Create(model), FadeIn(model_label), run_time=2.0, rate_func=smooth)
        self.play(Create(samples), FadeIn(sample_label), run_time=1.8, rate_func=smooth)
        self.wait(1.5)
