import math
import xml.etree.ElementTree as ET
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


def _fetch_tiger_svg():
    try:
        from pyodide.http import open_url
    except ImportError:
        from urllib.request import urlopen

        with urlopen(VELLO_TIGER_URL) as response:
            return response.read().decode("utf-8")
    return open_url(VELLO_TIGER_URL).read()


def _make_kaleidoscope_svg(source):
    """Keep every tiger leaf/style while moving its geometry into a pinwheel."""
    ET.register_namespace("", "http://www.w3.org/2000/svg")
    root = ET.fromstring(source)
    paths = [node for node in root.iter() if node.tag.rsplit("}", 1)[-1] == "path"]
    count = max(1, len(paths))
    for index, path in enumerate(paths):
        phase = index / count
        angle = 28.0 * math.sin(phase * math.tau * 3.0)
        radius = 7.0 + 5.0 * math.sin(phase * math.tau * 5.0) ** 2
        direction = phase * math.tau * 2.0
        dx = radius * math.cos(direction)
        dy = radius * math.sin(direction)
        transform = f"translate({dx:.3f} {dy:.3f}) rotate({angle:.3f} 100 100)"
        existing = path.attrib.get("transform", "").strip()
        path.set("transform", f"{existing} {transform}".strip())
    return ET.tostring(root, encoding="unicode")


_DEMO_DIR = FilePath(gettempdir())
_TIGER_PATH = _DEMO_DIR / "noon-vello-ghostscript-tiger.svg"
_TARGET_PATH = _DEMO_DIR / "noon-svg-morph-target.svg"
_TIGER_SOURCE = _fetch_tiger_svg()
_TIGER_PATH.write_text(_TIGER_SOURCE, encoding="utf-8")
_TARGET_PATH.write_text(_make_kaleidoscope_svg(_TIGER_SOURCE), encoding="utf-8")


class GhostscriptTigerMorph(Scene):
    def construct(self):
        tiger = SVGMobject(str(_TIGER_PATH), height=5.2)
        kaleidoscope = SVGMobject(str(_TARGET_PATH), height=5.2)
        tiger_return = SVGMobject(str(_TIGER_PATH), height=5.2)

        self.add(tiger)
        self.wait(0.5)
        self.play(Transform(tiger, kaleidoscope), run_time=1.8)
        self.wait(0.4)
        self.play(Transform(tiger, tiger_return), run_time=1.8)
        self.wait(0.5)
