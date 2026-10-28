"""Persistent and temporary glow pulses share ordinary timing and composition.

Paired with noon::example_scenes::glow_pulse. A pulse restores intensity at
its mapped child endpoint; an unadopted temporary binding retires at the
enclosing play's completion barrier.
"""
from noon import (
    BLUE, WHITE, LEFT, RIGHT, UP,
    Blur, Circle, GlowPulse, Pixels, Rectangle, Scene, linear,
)


class GlowPulseScene(Scene):
    def construct(self):
        background = Rectangle(width=16, height=9, fill="#08101f", fill_opacity=1, stroke=None)
        persistent = Circle(radius=0.4, fill=WHITE, fill_opacity=1, stroke=None).shift(2 * LEFT)
        temporary = Circle(radius=0.4, fill=WHITE, fill_opacity=1, stroke=None).shift(2 * RIGHT)
        persistent.set_glow(color=BLUE, radius=Pixels(10), intensity=0.35)
        # Neutral pixels, but a real existing binding: pulse cleanup must preserve it.
        temporary.add_effect(Blur(radius=Pixels(0)), name="soften")
        self.add(background, persistent, temporary)
        self.wait(0.5)

        self.play(persistent.animate.set_glow(intensity=1.2), run_time=0.5, rate_func=linear)
        self.play(
            GlowPulse(persistent, intensity=2),
            GlowPulse(temporary, intensity=2),
            persistent.animate(rate_func=linear).shift(0.4 * UP).set_glow(radius=Pixels(12)),
            run_time=0.6,
        )
        # Left restores 1.2 and keeps motion/radius; right keeps its neutral blur.
        assert temporary.get_effect("soften").authored_definition.radius == Pixels(0)
        self.wait(0.4)

        # Other bounded maps use the same lifecycle, including explicit reversal.
        self.play(GlowPulse(temporary, intensity=1.5, run_time=0.5, rate_func=linear))
        self.wait(0.25)
        self.play(GlowPulse(
            temporary, intensity=1.5, run_time=0.5,
            rate_func=linear, reverse_rate_function=True,
        ))
        self.wait(0.25)
