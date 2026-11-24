"""Treat a composed group, then dissolve and restore that same composition.

Uses the shared Rust paths demonstrated by example_scenes::{composed_effects,
ordered_effects, noise_dissolve}; the authored timeline owns every change.
"""
from noon import *


class LayeredEffects(Scene):
    def construct(self):
        muted = Color(0.62, 0.69, 0.82)
        self.add(Rectangle(width=16, height=9, fill=Color(0.035, 0.045, 0.09),
                           fill_opacity=1, stroke=None))
        self.add(Text("ONE COMPOSITION", font_size=36, color=WHITE).move_to((0, 3.1, 0)))
        self.add(Text("Filter the group, then dissolve it.", font_size=20, color=muted)
                 .move_to((0, 2.55, 0)))

        def motif(x):
            coral = Circle(radius=0.78, fill=Color(0.95, 0.4, 0.42), fill_opacity=1,
                           stroke=None).move_to((x - 0.45, -0.2, 0))
            teal = Rectangle(width=1.3, height=1.5, fill=Color(0.18, 0.78, 0.75),
                             fill_opacity=1, stroke=None).rotate(-0.2).move_to((x + 0.5, -0.1, 0))
            gold = Circle(radius=0.34, fill=Color(1, 0.82, 0.46), fill_opacity=1,
                          stroke=None).move_to((x + 0.35, 0.75, 0))
            return VGroup(coral, teal, gold)

        source, treated = motif(-3), motif(3)
        treated.add_effect(Blur(radius=Pixels(0)), name="soften", scope="composed")
        treated.set_glow(radius=Pixels(8), intensity=0, color=Color(0.35, 0.65, 1),
                         scope="composed")
        self.add(source, treated)
        self.add(Text("SOURCE", font_size=20, color=muted).move_to((-3, 1.7, 0)))
        self.add(Text("TREATED", font_size=20, color=muted).move_to((3, 1.7, 0)))
        self.add(Text("Blur → Glow → Dissolve", font_size=22, color=WHITE).move_to((0, -2.1, 0)))
        self.add(Text("Overlapping shapes stay one composition.", font_size=18, color=muted)
                 .move_to((0, -2.7, 0)))
        self.wait(0.6)
        self.play(treated.animate.set_effect("soften", radius=Pixels(2))
                  .set_effect("glow", intensity=1.6), run_time=2, rate_func=smooth)
        self.wait(0.8)
        self.play(Dissolve(treated, direction="out", seed=7, softness=0.2, remover=False),
                  run_time=1.2, rate_func=linear)
        self.wait(0.35)
        self.play(Dissolve(treated, direction="in", seed=7, softness=0.2, remover=False),
                  run_time=1.2, rate_func=linear)
        self.wait(0.35)
        self.play(treated.animate.set_effect("soften", radius=Pixels(0))
                  .set_effect("glow", intensity=0), run_time=1.5, rate_func=smooth)
        self.wait(1)
