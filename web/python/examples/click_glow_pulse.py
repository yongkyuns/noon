"""Click either shape to pulse its glow without advancing authored time.

The left shape restores its persistent blue glow. The right starts with no
glow, acquires a temporary attachment on click, and retires it on completion.
Paired with noon::example_scenes::click_glow_pulse.
"""
from noon import BLUE, WHITE, LEFT, RIGHT, Circle, GlowPulse, Pixels, Rectangle, Scene


class ClickGlowPulse(Scene):
    def construct(self):
        persistent = Circle(radius=0.55, fill=WHITE, fill_opacity=1, stroke=None).shift(2 * LEFT)
        temporary = Rectangle(width=1.1, height=1.1, fill=WHITE, fill_opacity=1, stroke=None).shift(2 * RIGHT)
        persistent.set_glow(color=BLUE, radius=Pixels(12), intensity=0.35)
        self.add(persistent, temporary)
        self.on_click(persistent, GlowPulse(persistent, intensity=2, run_time=1))
        self.on_click(temporary, GlowPulse(temporary, intensity=2, run_time=1))
