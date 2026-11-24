"""A standalone soundtrack audition for the Fourier epicycle demo.

Paste this file into https://yongkyuns.github.io/noon/, press Run, then Play.
This is a finite audio preview, not the animated epicycle scene: the current
deployed engine rejects AudioSource together with ordinary Scene.play().

Music: A Small Discovery, the original CC0-1.0 score bundled with Noon.
Soft keys and warm pads; no external file, URL, or additional dependency.
All motion and audio below use Noon's native finite timeline, with no updater.
"""

from noon import *


DURATION = 34.65
MUSIC_GAIN = 0.5  # Linear gain; Noon's viewer volume is a separate control.
FADE_IN = 1.5
FADE_OUT = 2.5

SVG_SOURCE = """<svg xmlns="http://www.w3.org/2000/svg" viewBox="-110 -105 220 190">
  <path d="M 0 -24
    C -10 -48 -24 -78 -47 -85
    C -91 -102 -105 -70 -95 -35
    C -90 -16 -76 -1 -57 8
    C -87 7 -89 34 -71 55
    C -54 76 -28 60 -9 30
    C -4 45 -5 61 0 75
    C 5 61 4 45 9 30
    C 28 60 54 76 71 55
    C 89 34 87 7 57 8
    C 76 -1 90 -16 95 -35
    C 105 -70 91 -102 47 -85
    C 24 -78 10 -48 0 -24 Z"/>
</svg>"""


class FourierMusicPreview(Scene):
    def construct(self):
        self.add(Text("A SMALL DISCOVERY", font_size=32).move_to((0, 3.1)))
        self.add(Text("Soundtrack preview for Fourier epicycles", font_size=19,
                      color=GRAY).move_to((0, 2.55)))

        butterfly = SVGMobject.from_string(SVG_SOURCE, height=3.4)
        butterfly.set_fill(opacity=0).set_stroke(TEAL, width=2.8)
        butterfly.move_to((0, 0.25))
        self.add(butterfly)

        self.add(Line((-3.8, -2.2), (3.8, -2.2), color=GRAY, stroke_width=1.2))
        marker = Dot((-3.8, -2.2), radius=0.065, color=TEAL)
        self.add(marker)
        self.add(Text("Soft keys and warm pads", font_size=20,
                      color=TEAL).move_to((0, -2.85)))
        self.add(Text("Original Noon score | CC0 | 34.65 seconds", font_size=15,
                      color=GRAY).move_to((0, -3.35)))

        # Finite scenes declare all membership before advancing authored time.
        progress = self.value_tracker(0)
        self.bind_position(marker, progress, direction=(7.6, 0), offset=(-3.8, -2.2))
        self.declare_value(progress, 1, run_time=DURATION, rate_func=linear)

        self.audio(
            AudioSource.small_discovery(),
            start_time=0.0,
            duration=DURATION,
            loop=DURATION > 80.0,
            gain=MUSIC_GAIN,
            fade_in=min(FADE_IN, DURATION / 2),
            fade_out=min(FADE_OUT, DURATION / 2),
        )
