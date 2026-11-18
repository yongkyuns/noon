"""Build a neon halo as a soft layer behind a sharp source."""
from noon import *

HALO_RADIUS = 18  # Try 8 or 24. The tube keeps its shape as the halo changes.
NIGHT = color_from_hex("#140f24")
PANEL = color_from_hex("#211930")
FRAME = color_from_hex("#57415f")
INK = color_from_hex("#fff5e9")
MUTED = color_from_hex("#d4bdd9")
CORE = color_from_hex("#ffe0f3")
HALO = color_from_hex("#ff4ab5")
GUIDE = color_from_hex("#ffcc8a")


def text(value, size, x, y, color=INK):
    return Text(value, font_size=size, color=color).move_to((x, y, 0))


def panel(x, width):
    return RoundedRectangle(width=width, height=3.45, corner_radius=0.22,
                            stroke=FRAME, stroke_width=1.6, fill=PANEL,
                            fill_opacity=1).move_to((x, 0.05, 0))


def tube(x, width=2.2, y=0.22, color=CORE):
    return Line((x - width / 2, y, 0), (x + width / 2, y, 0),
                color=color, stroke_width=9)


def sign(x):
    # A single stroked path carries the four tube letters through the GPU glow path.
    letters = VMobject(stroke_color=CORE, stroke_width=7, fill_opacity=0)
    glyphs = (
        ((0.18, 0), (0, 0.18), (0, 0.82), (0.18, 1), (0.82, 1),
         (1, 0.82), (1, 0.18), (0.82, 0), (0.18, 0)),
        ((0, 0), (0, 1), (0.78, 1), (1, 0.84), (1, 0.64), (0.78, 0.49), (0, 0.49)),
        ((1, 1), (0, 1), (0, 0), (1, 0)),
        ((0, 0), (0, 1), (1, 0), (1, 1)),
    )
    for index, glyph in enumerate(glyphs):
        left = x - 1.61 + 0.86 * index
        points = [(left + 0.64 * px, -0.23 + 1.06 * py, 0) for px, py in glyph]
        letters.start_new_path(points[0])
        for point in points[1:]:
            letters.add_line_to(point)
        if index == 2:
            letters.start_new_path((left, 0.3, 0))
            letters.add_line_to((left + 0.55, 0.3, 0))
    border = RoundedRectangle(width=4.5, height=2.05, corner_radius=0.38,
                              stroke=HALO, stroke_width=4, fill_opacity=0)
    border.move_to((x, 0.3, 0))
    return VGroup(letters, border)


class NeonGlow(Scene):
    def construct(self):
        if not 4 <= HALO_RADIUS <= 24:
            raise ValueError("Choose a halo radius between 4 and 24 pixels.")
        self.add(Rectangle(width=16, height=9, fill=NIGHT, fill_opacity=1, stroke=None))
        self.next_section("Light outside the tube")
        title = text("Where does the light spill come from?", 41, 0, 3.13)
        left, right = sign(-3.25), sign(3.25)
        hook_parts = [
            title, panel(-3.25, 5.65), panel(3.25, 5.65), left, right,
            text("Sharp core only", 28, -3.25, -0.96, MUTED),
            text("Core + halo", 28, 3.25, -0.96, MUTED),
            text("Look at the space around the letters", 29, 0, -2.81, GUIDE),
        ]
        self.add(*hook_parts)
        self.play(*(part.animate.set_glow(radius=Pixels(HALO_RADIUS), intensity=2.2, color=HALO)
                    for part in right), run_time=1.1)
        self.wait(1.9)

        self.next_section("Build the light in two layers")
        heading = text("Take one piece of the tube", 41, 0, 3.13)
        cards = [panel(x, 3.82) for x in (-4.42, 0, 4.42)]
        core = tube(-4.42)
        spread = tube(0, color=HALO)
        spread.add_effect(Blur(radius=Pixels(0)), name="spread")
        combined = tube(4.42)
        combined.set_glow(radius=Pixels(HALO_RADIUS), intensity=2.2, color=HALO)
        core_label = text("Sharp core", 29, -4.42, -1.05)
        spread_label = text("Soft light", 29, 0, -1.05)
        combined_label = text("Together", 29, 4.42, -1.05)
        plus = text("+", 37, -2.21, 0.22, GUIDE)
        equals = text("=", 37, 2.21, 0.22, GUIDE)
        self.remove(title, hook_parts[-1])
        self.play(*(FadeOut(item) for item in hook_parts[1:-1]), FadeIn(heading), run_time=0.6)
        self.play(*(FadeIn(item) for item in cards), FadeIn(core_label), Create(core), run_time=0.6)
        self.wait(1.6)

        blur_caption = text("Spread a copy of the tube's light", 31, 0, -2.79, GUIDE)
        self.play(FadeIn(plus), FadeIn(spread), FadeIn(blur_caption), run_time=0.4)
        self.play(spread.animate.set_effect("spread", radius=Pixels(HALO_RADIUS)),
                  FadeIn(spread_label), run_time=0.8)
        self.wait(1.6)
        self.remove(blur_caption)
        layering = text("Put the sharp core back on top", 31, 0, -2.79, GUIDE)
        self.play(FadeIn(equals), FadeIn(combined), FadeIn(combined_label), FadeIn(layering),
                  run_time=1.4)
        self.wait(2.3)

        self.next_section("Watch the gap")
        self.remove(heading, layering)
        gap_heading = text("What happens in the gap?", 41, 0, 3.13)
        gap_card = panel(0, 8.1)
        high, low = tube(0, width=4.4, y=0.84), tube(0, width=4.4, y=-0.84)
        for source in (high, low):
            source.set_glow(radius=Pixels(HALO_RADIUS), intensity=2.2, color=HALO)
        self.play(*(FadeOut(item) for item in [*cards, core, core_label, spread, spread_label,
                                              combined, combined_label, plus, equals]),
                  FadeIn(gap_heading), run_time=0.6)
        self.play(FadeIn(gap_card), FadeIn(high), FadeIn(low), run_time=0.6)
        self.play(high.animate.shift((0, -0.64, 0)), low.animate.shift((0, 0.64, 0)), run_time=1.8)
        gap_caption = text("The two soft halos overlap in the gap", 30, 0, -2.79, GUIDE)
        self.play(FadeIn(gap_caption), run_time=0.4)
        self.wait(1.6)

        self.next_section("Return to the sign")
        self.remove(gap_heading, gap_caption)
        final = sign(0).scale(1.26)
        for part in final:
            part.set_glow(radius=Pixels(HALO_RADIUS), intensity=2.2, color=HALO)
        final_heading = text("Sharp tubing. Soft light.", 43, 0, 3.13)
        conclusion = text("The halo reaches beyond the source", 31, 0, -2.79, GUIDE)
        self.play(FadeOut(gap_card), FadeOut(high), FadeOut(low), FadeIn(final),
                  FadeIn(final_heading), FadeIn(conclusion), run_time=1.3)
        self.wait(3.6)
