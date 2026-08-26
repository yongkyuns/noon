from pathlib import Path

UPSTREAM = """from manim import *

class MovingDots(Scene):
    def construct(self):
        d1,d2=Dot(color=BLUE),Dot(color=GREEN)
        dg=VGroup(d1,d2).arrange(RIGHT,buff=1)
        l1=Line(d1.get_center(),d2.get_center()).set_color(RED)
        x=ValueTracker(0)
        y=ValueTracker(0)
        d1.add_updater(lambda z: z.set_x(x.get_value()))
        d2.add_updater(lambda z: z.set_y(y.get_value()))
        l1.add_updater(lambda z: z.match_points(Line(d1.get_center(),d2.get_center())))
        self.add(d1,d2,l1)
        self.play(x.animate.set_value(5))
        self.play(y.animate.set_value(4))
        self.wait()
"""

SVG = """<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 320 180\" role=\"img\" aria-label=\"Blue and green dots connected by a red updater line\">
  <rect width=\"320\" height=\"180\" fill=\"#1c1c1c\"/>
  <line x1=\"250\" y1=\"104\" x2=\"174\" y2=\"42\" stroke=\"#fc6255\" stroke-width=\"4\" stroke-linecap=\"round\"/>
  <circle cx=\"250\" cy=\"104\" r=\"7\" fill=\"#58c4dd\"/>
  <circle cx=\"174\" cy=\"42\" r=\"7\" fill=\"#83c167\"/>
</svg>
"""

Path("parity/manim-v0.21/upstream-examples/moving_dots.py").write_text(UPSTREAM)
Path("web/python/examples/manim_gallery_moving_dots.py").write_text(
    UPSTREAM.replace("from manim import *", "from noon import *", 1)
)
Path("web/thumbnails/manim/moving-dots.svg").write_text(SVG)

quickstart = Path("parity/manim-v0.21/quickstart.py")
text = quickstart.read_text()
if "class MovingDots(Scene):" not in text:
    body = UPSTREAM.split("\n", 2)[2]
    quickstart.write_text(text.rstrip() + "\n\n\n" + body.rstrip() + "\n")

parity = Path("parity/manim-v0.21/manifest.json")
text = parity.read_text()
if '"id": "moving-dots"' not in text:
    marker = '\n  ],\n  "sample_fractions"'
    if marker not in text:
        raise SystemExit("parity manifest insertion marker not found")
    pos = text.index(marker)
    fixture = '''    {
      "id": "moving-dots",
      "scene": "MovingDots",
      "expected_duration": 3.0
    }'''
    parity.write_text(text[:pos].rstrip() + ",\n" + fixture + text[pos:])

manifest = Path("web/python/examples/manim_tutorial_manifest.json")
text = manifest.read_text()
if '"id": "parity-moving-dots"' not in text:
    marker = '    {\n      "id": "text-and-math",'
    if marker not in text:
        raise SystemExit("demo manifest insertion marker not found")
    entry = '''    {
      "id": "parity-moving-dots",
      "title": "MovingDots",
      "summary": "ManimCE v0.21 Example Gallery MovingDots, byte-for-byte except for the import module.",
      "status": "ready",
      "path": "python/examples/manim_gallery_moving_dots.py",
      "category": "animations",
      "features": ["Dot", "VGroup", "arrange", "Line", "ValueTracker", "add_updater", "match_points", "pixel-parity", "time-parity"],
      "upstream": "examples.html",
      "upstream_source": "parity/manim-v0.21/upstream-examples/moving_dots.py",
      "reuse": "source-equivalent-manim-v0.21",
      "parity_status": "candidate",
      "parity_fixture": "moving-dots",
      "expected_duration": 3.0,
      "thumbnail": "thumbnails/manim/moving-dots.svg",
      "thumbnail_alt": "Blue and green dots connected by a red updater line",
      "thumbnail_time": 2.5,
      "order": 200
    },
'''
    manifest.write_text(text.replace(marker, entry + marker, 1))
