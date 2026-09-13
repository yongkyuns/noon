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

SVG_MORPH_SOURCE = """<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100">
  <path d="M10 50 L50 10 L90 50 L50 90 Z" fill="#58C4DD"/>
</svg>"""

SVG_MORPH_TARGET = """<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100">
  <path d="M15 25 L85 25 L85 75 L15 75 Z" fill="#58C4DD"/>
</svg>"""


class SvgStaticImport(Scene):
    def construct(self):
        path = Path("/tmp/noon_svg_static_parity.svg")
        path.write_text(SVG_SOURCE)
        icon = SVGMobject(str(path))
        self.add(icon)
        self.wait(0.2)


class SvgTransform(Scene):
    def construct(self):
        source_path = Path("/tmp/noon_svg_morph_source.svg")
        target_path = Path("/tmp/noon_svg_morph_target.svg")
        source_path.write_text(SVG_MORPH_SOURCE)
        target_path.write_text(SVG_MORPH_TARGET)

        source = SVGMobject(str(source_path))
        target = SVGMobject(str(target_path))
        self.add(source)
        self.play(Transform(source, target), run_time=1.0, rate_func=linear)
