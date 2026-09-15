"""Executable ManimCE oracle for the gallery tiger's real ordered filled Transform.

Reads the exact SVG constants from the gallery without executing/importing Noon.
Produces a reference movie, deterministic frames, and geometry/paint observations.
The Ghostscript artwork is fetched at the existing pinned revision, never bundled.
"""
from __future__ import annotations

import ast
import hashlib
import json
from pathlib import Path
import re
import subprocess
from urllib.request import urlopen

import manim
from manim import BLACK, Camera, SVGMobject, Transform, tempconfig, linear
import numpy as np
from PIL import Image

ROOT = Path(__file__).resolve().parents[1]
OUT = ROOT / "browser-smoke-artifacts/tiger-manim-reference"
OUT.mkdir(parents=True, exist_ok=True)
assert manim.__version__ == "0.21.0", manim.__version__
module = ast.parse((ROOT / "web/python/examples/manim_compatible_svg_tiger_morph.py").read_text())
constants = {}
for statement in module.body:
    if isinstance(statement, ast.Assign) and len(statement.targets) == 1:
        name = getattr(statement.targets[0], "id", "")
        if name in {"VELLO_TIGER_URL", "UNRELATED_TARGET_SVG", "_PARSE_ONLY_STROKE", "MORPH_STROKE_WIDTH"}:
            constants[name] = ast.literal_eval(statement.value)

def normalize_tags(source: str) -> str:
    def normalize(match: re.Match[str]) -> str:
        tag = match.group(0)
        if re.search(r"\bstroke\s*=", tag):
            return tag
        suffix = "/>" if tag.endswith("/>") else ">"
        return tag[:-len(suffix)] + constants["_PARSE_ONLY_STROKE"] + suffix
    return re.sub(r"<path\b[^>]*>", normalize, source)

with urlopen(constants["VELLO_TIGER_URL"], timeout=60) as response:
    raw_tiger = response.read()
tiger_path = OUT / "tiger.svg"
rocket_path = OUT / "rocket.svg"
tiger_path.write_text(normalize_tags(raw_tiger.decode()))
rocket_path.write_text(normalize_tags(constants["UNRELATED_TARGET_SVG"]))

def make_objects():
    tiger = SVGMobject(str(tiger_path), height=5.2)
    rocket = SVGMobject(str(rocket_path), height=5.2)
    for family in [tiger, rocket]:
        for leaf in family:
            if float(leaf.get_fill_opacity()) == 0.0:
                leaf.set_fill(BLACK, opacity=0)
            leaf.set_stroke(width=constants["MORPH_STROKE_WIDTH"])
    return tiger, rocket

def paint(mobject):
    return {"fill": mobject.get_fill_opacities().tolist(),
            "stroke": mobject.get_stroke_opacities().tolist(),
            "fill_rgbas": mobject.fill_rgbas.tolist(),
            "stroke_rgbas": mobject.stroke_rgbas.tolist(),
            "stroke_width": float(mobject.get_stroke_width())}

def capture(camera, mobject):
    camera.reset()
    camera.capture_mobjects([mobject])
    return np.array(camera.pixel_array, copy=True)

report = {"manim_version": manim.__version__, "renderer": "cairo",
          "asset_url": constants["VELLO_TIGER_URL"],
          "asset_sha256": hashlib.sha256(raw_tiger).hexdigest(),
          "samples": [], "notes": "Ordinary Transform; no outline phase, repaint, or replacement."}
with tempconfig({"renderer": "cairo", "pixel_width": 960, "pixel_height": 540,
                 "frame_width": 128/9, "frame_height": 8, "background_color": "#000000"}):
    # Independent placement oracles: dimension bounds contain handles but center
    # is anchored at the endpoints. The visible bulge need not be centered.
    for name, commands, width in [
        ('cubic', 'M0 0 C0 12 12 12 12 0 Z', 2.0),
        ('quadratic', 'M0 0 Q6 12 12 0 Z', 3.0),
    ]:
        fixture = OUT / f'{name}-placement.svg'
        fixture.write_text(f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 12 12"><path d="{commands}"/></svg>')
        shape = SVGMobject(str(fixture), height=2.0)
        assert abs(float(shape.width)-width) < 1e-8
        assert abs(float(shape.height)-2.0) < 1e-8
        assert np.max(np.abs(shape.get_center())) < 1e-8
        assert abs(float(shape.get_all_points()[:,1].max())) < 1e-8
        assert abs(float(shape.get_all_points()[:,1].min())+2.0) < 1e-8
    tiger, rocket = make_objects()
    saved = tiger.copy()
    report["source_leaves"] = len(tiger)
    report["target_leaves"] = len(rocket)
    n, m = len(tiger), len(rocket)
    mapping = (np.arange(n) * m // n).tolist()
    first_indices = [mapping.index(i) for i in range(m)]
    report["real_target_occurrences"] = first_indices
    report["target_padding_count"] = n - m
    camera = Camera()
    original = capture(camera, tiger)
    Image.fromarray(original).save(OUT / "original.png")
    for direction, target in [("forward", rocket), ("return", saved)]:
        animation = Transform(tiger, target, run_time=1.8)
        animation.begin()
        for alpha in [0.0, 0.01, 0.25, 0.5, 0.75, 0.99, 1.0]:
            animation.interpolate(alpha)
            pixels = capture(camera, tiger)
            label = f"{direction}-{alpha:.2f}"
            Image.fromarray(pixels).save(OUT / f"{label}.png")
            report["samples"].append({"direction": direction, "alpha": alpha,
                "eased_alpha": float(animation.rate_func(alpha)),
                "file": f"{label}.png", "leaf_count": len(tiger),
                "paints": [paint(leaf) for leaf in tiger],
                "points_sha256": hashlib.sha256(b"".join(leaf.points.tobytes() for leaf in tiger)).hexdigest(),
                "foreground_pixels": int(np.any(np.abs(pixels[:,:,:3].astype(int))>12, axis=2).sum())})
        animation.finish()
    restored = capture(camera, tiger)
    report["restoration_changed_pixels"] = int(np.any(np.abs(original.astype(int)-restored.astype(int))>12, axis=2).sum())
    assert report["restoration_changed_pixels"] < 100, report["restoration_changed_pixels"]

    # A minimal opacity counterexample isolates family padding from SVG parsing.
    from manim import Square, VGroup
    source = VGroup(*[Square(fill_opacity=a, stroke_opacity=0) for a in [1, 0, 1]])
    target = VGroup(*[Square(fill_opacity=1, stroke_opacity=0) for _ in range(2)])
    animation = Transform(source, target, rate_func=linear)
    animation.begin()
    animation.interpolate(0.5)
    report["padding_opacity_counterexample"] = [float(x.get_fill_opacity()) for x in source]
    assert report["padding_opacity_counterexample"] == [1.0, 0.0, 1.0]

    # Render a complete real animation, not a slideshow of the observations above.
    tiger, rocket = make_objects()
    saved = tiger.copy()
    ffmpeg = subprocess.Popen(["ffmpeg", "-y", "-loglevel", "error", "-f", "rawvideo",
        "-pix_fmt", "rgba", "-s", "960x540", "-r", "30", "-i", "-", "-an",
        "-c:v", "libx264", "-pix_fmt", "yuv420p", "-movflags", "+faststart",
        str(OUT / "manim-tiger-rocket-roundtrip.mp4")], stdin=subprocess.PIPE)
    try:
        def write_frame():
            ffmpeg.stdin.write(capture(camera, tiger).tobytes())
        for _ in range(15): write_frame()
        for target, hold_frames in [(rocket, 23), (saved, 26)]:
            animation = Transform(tiger, target, run_time=1.8)
            animation.begin()
            for frame in range(54):
                animation.interpolate(frame / 54)
                write_frame()
            animation.finish()
            for _ in range(hold_frames): write_frame()
    finally:
        ffmpeg.stdin.close()
        code = ffmpeg.wait(timeout=60)
    assert code == 0, code
report["outcome"] = "pass"
(OUT / "reference.json").write_text(json.dumps(report, indent=2))
print(json.dumps({k: v for k,v in report.items() if k != "samples"}, indent=2))
