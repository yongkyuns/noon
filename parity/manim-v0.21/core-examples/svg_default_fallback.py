from manim import *
from pathlib import Path


SVG_SOURCE = """<svg xmlns="http://www.w3.org/2000/svg">
  <rect x="5" y="5" width="40" height="50"/>
  <circle cx="75" cy="30" r="20" fill="#58C4DD"/>
  <path d="M105 5 L150 30 L105 55 Z"
        style="fill:#FFFFFF;stroke:#FC6255;stroke-opacity:0.5;stroke-width:3"/>
</svg>"""

SVG_DEFAULT = {
    "color": "#9C27B0",
    "opacity": 0.35,
    "fill_color": "#FC6255",
    "fill_opacity": None,
    "stroke_width": 6,
    "stroke_color": "#83C167",
    "stroke_opacity": 0.85,
}


class SvgDefaultFallback(Scene):
    def construct(self):
        path = Path("/tmp/noon_svg_default_parity.svg")
        path.write_text(SVG_SOURCE)
        icon = SVGMobject(str(path), svg_default=SVG_DEFAULT)
        self.add(icon)
        self.wait(0.2)
