import math
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


_DEMO_DIR = FilePath(gettempdir())
_TIGER_PATH = _DEMO_DIR / "noon-vello-ghostscript-tiger.svg"
_TIGER_PATH.write_text(_fetch_tiger_svg(), encoding="utf-8")


class GhostscriptTigerMorph(Scene):
    def construct(self):
        tiger = SVGMobject(str(_TIGER_PATH), height=5.2)
        kaleidoscope = SVGMobject(str(_TIGER_PATH), height=5.2)
        tiger_return = SVGMobject(str(_TIGER_PATH), height=5.2)

        count = max(1, len(kaleidoscope))
        for index, leaf in enumerate(kaleidoscope):
            phase = index / count
            angle = 32.0 * math.sin(phase * math.tau * 3.0)
            radius = 0.35 + 0.45 * math.sin(phase * math.tau * 5.0) ** 2
            direction = phase * math.tau * 2.0
            offset = RIGHT * (radius * math.cos(direction)) + UP * (
                radius * math.sin(direction)
            )
            leaf.rotate(angle * DEGREES).shift(offset)

        self.add(tiger)
        self.wait(0.5)
        self.play(Transform(tiger, kaleidoscope), run_time=1.8)
        self.wait(0.4)
        self.play(Transform(tiger, tiger_return), run_time=1.8)
        self.wait(0.5)
