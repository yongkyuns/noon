"""Light as a property: animate Gaussian glow through the ordinary Scene API.

The background and labels are ordinary scene objects. Circles, a stroked path,
and mathematical glyphs own glow attachments in the shared Rust semantic/runtime path;
there is no Python updater, separate effect renderer, or frame-rate-dependent
interpolation. A later beat retires the first generation and re-enrolls it.
"""
from noon import *


class GpuGlowShowcase(Scene):
    def construct(self):
        night = Color(0.025, 0.035, 0.075)
        muted = Color(0.58, 0.68, 0.82)
        background = Rectangle(
            width=16.0, height=9.0, fill=night,
            fill_opacity=1.0, stroke=None,
        )
        heading = Text("GPU GLOW", font_size=39, color=WHITE).shift(3.0 * UP)
        subtitle = Text("One shape. Three ways to change the light.", font_size=20, color=muted).shift(2.44 * UP)
        positions = (-3.5, 0.0, 3.5)
        names = ("INTENSITY", "RADIUS", "COLOR")
        label_colors = (BLUE, GREEN, PINK)
        labels = [
            Text(name, font_size=22, color=color).move_to((x, -1.72, 0))
            for x, name, color in zip(positions, names, label_colors)
        ]
        orbs = [
            Circle(radius=0.58, fill=WHITE, fill_opacity=1.0, stroke=None)
            .move_to((x, 0.2, 0))
            for x in positions
        ]

        self.add(background)
        self.play(FadeIn(heading), FadeIn(subtitle), *[FadeIn(label) for label in labels],
                  run_time=0.8, rate_func=smooth)
        self.play(*[FadeIn(orb) for orb in orbs], run_time=0.7, rate_func=smooth)

        # These targets have no prior glow. Rust enrolls a neutral attachment
        # and drives its intensity from zero in the normal compiled timeline.
        self.play(
            orbs[0].animate.set_glow(color=BLUE, radius=Pixels(7), intensity=1.5),
            orbs[1].animate.set_glow(color=GREEN, radius=Pixels(4), intensity=1.5),
            orbs[2].animate.set_glow(color=BLUE, radius=Pixels(7), intensity=1.4),
            run_time=1.4, rate_func=smooth,
        )
        self.wait(0.5)

        # Only the named glow parameter changes; the underlying geometry and
        # painter order stay put. The hold is the gallery's genuine poster frame.
        self.play(
            orbs[0].animate.set_glow(intensity=2.3),
            orbs[1].animate.set_glow(radius=Pixels(16)),
            orbs[2].animate.set_glow(color=PINK),
            run_time=1.4, rate_func=smooth,
        )
        self.wait(1.0)

        self.play(*[orb.animate.set_glow(intensity=0.0) for orb in orbs],
                  run_time=1.3, rate_func=smooth)
        self.wait(0.35)
        for orb in orbs:
            orb.remove_glow()

        # A fresh generation lights the same sources again. Removal and
        # re-entry exercise attachment lifecycle rather than hiding an old halo.
        self.play(
            orbs[0].animate.set_glow(color=BLUE, radius=Pixels(8), intensity=2.0),
            orbs[1].animate.set_glow(color=GREEN, radius=Pixels(12), intensity=1.7),
            orbs[2].animate.set_glow(color=PINK, radius=Pixels(9), intensity=1.6),
            run_time=1.25, rate_func=smooth,
        )
        self.wait(0.8)

        # Stroked vector geometry and MathTypst use the same retained glow
        # path. Reveal both sources while their independent glow tracks animate.
        path = VMobject(stroke_color=BLUE, stroke_width=5, fill_opacity=0)
        path.start_new_path(2.4 * LEFT + 2.4 * DOWN)
        path.add_cubic_bezier_curve_to(1.2 * LEFT + 1.6 * DOWN, 1.2 * RIGHT + 3.0 * DOWN, 2.4 * RIGHT + 2.1 * DOWN)
        equation = MathTypst("integral_0^1 x^2 dif x = frac(1, 3)", font_size=26).shift(3.15 * DOWN)
        path.set_glow(color=BLUE, radius=Pixels(8), intensity=1.0)
        equation.set_glow(color=PINK, radius=Pixels(7), intensity=0.9)
        self.play(
            Create(path), Write(equation),
            path.animate.set_glow(intensity=1.7, radius=Pixels(12)),
            equation.animate.set_glow(intensity=1.4, color=BLUE),
            run_time=1.5, rate_func=smooth,
        )
        self.wait(1.0)
