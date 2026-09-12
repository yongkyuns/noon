from manim import *
from pathlib import Path


SVG_SOURCE = """<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 120 60">
  <rect x="5" y="5" width="40" height="50"
        fill="#58C4DD" stroke="#FFFFFF" stroke-width="2"/>
  <circle cx="75" cy="30" r="20" fill="#83C167"/>
  <g transform="translate(100 10)">
    <path d="M0 0 L15 20 L0 40 Z" fill="#FC6255"/>
  </g>
</svg>"""


class SvgStaticImport(Scene):
    def construct(self):
        path = Path("/tmp/noon_svg_static_parity.svg")
        path.write_text(SVG_SOURCE)
        icon = SVGMobject(str(path))
        self.add(icon)
        self.wait(0.2)
