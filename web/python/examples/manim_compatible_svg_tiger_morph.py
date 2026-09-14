import re
from pathlib import Path as FilePath
from tempfile import gettempdir

from noon import *


# Vello's Ghostscript Tiger demo asset, pinned to the Vello commit current when
# this example was added. The artwork is by the Ghostscript authors and is
# AGPL-3.0-or-later; it is fetched only when this example runs and is not bundled
# into Noon's normal authoring/runtime payload.
VELLO_TIGER_URL = (
    "https://cdn.jsdelivr.net/gh/linebender/vello@"
    "1e63b4a40ccb484f82e1d85b83df97ab95bcfbe7/assets/Ghostscript_Tiger.svg"
)

# Intentionally unrelated to the Tiger family: six independently parsed filled
# SVG paths form a rocket. Keeping the target path-only stays inside the generic
# SVG Transform qualification while exercising a very different family size and
# topology from the imported Tiger.
UNRELATED_TARGET_SVG = """<svg xmlns="http://www.w3.org/2000/svg" viewBox="-180 -180 360 360">
  <path d="M0 -145 C58 -98 72 -4 40 80 L-40 80 C-72 -4 -58 -98 0 -145 Z" fill="#58C4DD"/>
  <path d="M-40 55 L-100 112 L-38 96 Z" fill="#FC6255"/>
  <path d="M40 55 L100 112 L38 96 Z" fill="#FC6255"/>
  <path d="M0 -44 C25 -44 44 -25 44 0 C44 25 25 44 0 44 C-25 44 -44 25 -44 0 C-44 -25 -25 -44 0 -44 Z" fill="#236B8E"/>
  <path d="M-24 80 L0 150 L24 80 Z" fill="#FF862F"/>
  <path d="M-10 82 L0 122 L10 82 Z" fill="#FFFF00"/>
</svg>"""

# Retained path morphing keeps tessellation-affecting style topology fixed.
# The SVG importer maps its 0.000001 parse-only sentinel back to semantic zero
# width. Give only stroke-less path tags an invisible stroke at that sentinel so
# usvg retains SVG-default miter/butt structure without changing visible paint.
_PARSE_ONLY_STROKE = ' stroke="#000" stroke-width="0.000001" stroke-opacity="0"'
MORPH_STROKE_WIDTH = 0.4


def _fetch_tiger_svg():
    try:
        from pyodide.http import open_url
    except ImportError:
        from urllib.request import urlopen

        with urlopen(VELLO_TIGER_URL) as response:
            return response.read().decode("utf-8")
    return open_url(VELLO_TIGER_URL).read()


def _with_parse_only_strokes(source):
    def normalize_path(match):
        tag = match.group(0)
        if re.search(r"\bstroke\s*=", tag):
            return tag
        suffix = "/>" if tag.endswith("/>") else ">"
        return tag[: -len(suffix)] + _PARSE_ONLY_STROKE + suffix

    return re.sub(r"<" + r"path\b[^>]*>", normalize_path, source)


def _normalize_morph_style(family):
    for leaf in family:
        if leaf.style.get("fill") is None:
            leaf.set_fill(BLACK, opacity=0)
        leaf.set_stroke(width=MORPH_STROKE_WIDTH)


_DEMO_DIR = FilePath(gettempdir())
_TIGER_PATH = _DEMO_DIR / "noon-vello-ghostscript-tiger.svg"
_TARGET_PATH = _DEMO_DIR / "noon-unrelated-rocket-target.svg"
_TIGER_PATH.write_text(_with_parse_only_strokes(_fetch_tiger_svg()), encoding="utf-8")
_TARGET_PATH.write_text(_with_parse_only_strokes(UNRELATED_TARGET_SVG), encoding="utf-8")


class GhostscriptTigerMorph(Scene):
    def construct(self):
        tiger = SVGMobject(str(_TIGER_PATH), height=5.2)
        rocket = SVGMobject(str(_TARGET_PATH), height=5.2)
        _normalize_morph_style(tiger)
        _normalize_morph_style(rocket)

        # Complex filled paths intentionally stay within Noon's bounded safe-fill
        # morph contract. Perform the unrelated geometry deformation as an outline,
        # where shared path alignment can pad differing contour counts without
        # inventing per-frame topology, then restore each family's authored fills.
        tiger_return = tiger.copy()
        tiger_return.set_fill(opacity=0)
        rocket.set_fill(opacity=0)
        rocket.set_stroke(BLACK, width=MORPH_STROKE_WIDTH, opacity=1)

        self.add(tiger)
        self.wait(0.5)
        self.play(
            tiger.animate.set_fill(opacity=0).set_stroke(
                BLACK, width=MORPH_STROKE_WIDTH, opacity=1
            ),
            run_time=0.35,
        )
        self.play(Transform(tiger, rocket), run_time=1.8)
        self.play(tiger.animate.set_fill(opacity=1), run_time=0.35)
        self.wait(0.4)
        self.play(tiger.animate.set_fill(opacity=0), run_time=0.35)
        self.play(Transform(tiger, tiger_return), run_time=1.8)
        self.play(tiger.animate.set_fill(opacity=1), run_time=0.35)
        self.wait(0.5)
