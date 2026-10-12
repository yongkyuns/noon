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

TRANSFORM_SOURCE_SVG = """<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 120 60">
  <path d="M8 50 L24 10 L40 50 Z" fill="#58C4DD"/>
  <path d="M64 48 L78 12 L92 48 Z" fill="#83C167"/>
</svg>"""

TRANSFORM_TARGET_SVG = """<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 120 60">
  <path d="M10 46 L22 14 L34 46 Z" fill="#FC6255"/>
  <path d="M48 48 L62 12 L76 48 Z" fill="#58C4DD"/>
  <path d="M86 46 L100 14 L114 46 Z" fill="#83C167"/>
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
        source_path = Path("/tmp/noon_svg_transform_source.svg")
        target_path = Path("/tmp/noon_svg_transform_target.svg")
        source_path.write_text(TRANSFORM_SOURCE_SVG)
        target_path.write_text(TRANSFORM_TARGET_SVG)

        source = SVGMobject(str(source_path))
        target = SVGMobject(str(target_path))

        self.add(source)
        self.play(Transform(source, target), run_time=1.0, rate_func=linear)
