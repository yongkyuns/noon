"""A finished motion-design composition using the existing paints and backdrop.

Layout and type stay editable while ordinary tracks animate color, grain and
frosted glass. No generated images, per-frame callbacks or new effect API.
"""
from noon import *


class EditorialCover(Scene):
    def construct(self):
        paper, ink, muted = color_from_hex("#f1ede3"), color_from_hex("#172136"), color_from_hex("#68707a")
        self.add(Rectangle(width=16, height=9, fill=paper, fill_opacity=1, stroke=None))
        self.add(Text("FIELD / 01", font_size=16, color=ink).move_to((-5.35, 3.3, 0)))
        self.add(Text("A NOON MOTION STUDY", font_size=14, color=muted).move_to((3.4, 3.3, 0)))
        for word, y in [("MAKE", 1.65), ("IT", 0.2), ("MOVE", -1.25)]:
            self.add(Text(word, font_size=76, color=ink).move_to((-3.1, y, 0)))
        self.add(Text("Art-direct the surface.\nKeep the layout.", font_size=20, color=muted).move_to((-3.1, -2.85, 0)))

        stops = [(0, color_from_hex("#ffd890")), (0.38, color_from_hex("#ed795c")), (0.72, color_from_hex("#78578c")), (1, color_from_hex("#19375f"))]
        start = RadialGradient(stops, center=(0.15, 0.18), radii=(0.95, 1.1))
        finish = RadialGradient(stops, center=(0.85, 0.75), radii=(0.8, 0.95))
        surface = Rectangle(width=4.6, height=5.5, stroke=None).set_fill(start, opacity=1).move_to((3.4, 0, 0))
        grain_start = NoisePaint(BLACK, WHITE, seed=21, frequency=(180, 220), phase=(0, 0))
        grain_finish = NoisePaint(BLACK, WHITE, seed=21, frequency=(180, 220), phase=(0.7, -0.4))
        grain = Rectangle(width=4.6, height=5.5, stroke=None).set_fill(grain_start, opacity=0.09).move_to((3.4, 0, 0))
        self.add(surface, grain)
        for radius in (0.7, 1.15, 1.6):
            self.add(Circle(radius=radius, color=paper, stroke_width=1.3, fill_opacity=0).move_to((3.4, 0.4, 0)))
        pane = Rectangle(width=3.8, height=1.0, fill=WHITE, fill_opacity=0.16, stroke=None).move_to((3.4, -1.8, 0))
        heading = Text("COLOR / GRAIN / GLASS", font_size=16, color=WHITE).move_to((3.4, -1.64, 0))
        detail = Text("A cover in motion", font_size=15, color=WHITE).move_to((3.4, -1.98, 0))
        card = VGroup(pane, heading, detail)
        card.add_effect(BackdropBlur(radius=Pixels(4)), name="glass")
        self.add(card)
        self.add(Line((-6.2, -3.5, 0), (6.2, -3.5, 0), color=muted, stroke_width=0.8))
        self.add(Text("ANIMATED IDENTITY", font_size=13, color=ink).move_to((-4.65, -3.73, 0)))
        self.add(Text("FORM, TEXTURE, HIERARCHY", font_size=13, color=muted).move_to((3.8, -3.73, 0)))
        self.next_section("Establish the layout")
        self.wait(0.7)
        self.next_section("Animate color, grain and glass")
        self.play(surface.animate.set_fill(finish), grain.animate.set_fill(grain_finish, opacity=0.09),
                  card.animate.shift((-0.35, 0.15, 0)).set_effect("glass", radius=Pixels(12)), run_time=3, rate_func=smooth)
        self.next_section("Inspect the surface")
        self.wait(0.8)
        self.next_section("Return to the starting design")
        self.play(surface.animate.set_fill(start), grain.animate.set_fill(grain_start, opacity=0.09),
                  card.animate.shift((0.35, -0.15, 0)).set_effect("glass", radius=Pixels(4)), run_time=3, rate_func=smooth)
        self.wait(1.5)
