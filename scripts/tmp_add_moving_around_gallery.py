from pathlib import Path

manifest_path = Path("web/python/examples/manim_tutorial_manifest.json")
text = manifest_path.read_text(encoding="utf-8")

if '"id": "parity-moving-around"' in text:
    raise SystemExit("MovingAround manifest entry already exists")

marker = '''    {
      "id": "text-and-math",'''
entry = '''    {
      "id": "parity-moving-around",
      "title": "MovingAround",
      "summary": "ManimCE v0.21 Example Gallery MovingAround, byte-for-byte except for the import module, with Rust and JavaScript semantic equivalents.",
      "status": "ready",
      "path": "python/examples/manim_gallery_moving_around.py",
      "category": "animations",
      "features": ["Square", "animate", "shift", "set_fill", "scale", "rotate", "tri-language-parity"],
      "upstream": "examples.html",
      "upstream_source": "parity/manim-v0.21/upstream-examples/moving_around.py",
      "reuse": "source-equivalent-manim-v0.21",
      "parity_status": "candidate",
      "expected_duration": 4.0,
      "thumbnail": "thumbnails/manim/exact-source.svg",
      "thumbnail_alt": "Exact-source Manim MovingAround",
      "thumbnail_time": 2.0,
      "order": 220
    },
'''

if marker not in text:
    raise SystemExit("manifest insertion marker not found")
manifest_path.write_text(text.replace(marker, entry + marker, 1), encoding="utf-8")

upstream = Path("parity/manim-v0.21/upstream-examples/moving_around.py").read_text(encoding="utf-8")
public = Path("web/python/examples/manim_gallery_moving_around.py").read_text(encoding="utf-8")
if upstream.count("from manim import *") != 1:
    raise SystemExit("upstream fixture must contain exactly one Manim star import")
if public != upstream.replace("from manim import *", "from noon import *", 1):
    raise SystemExit("Noon MovingAround source differs from upstream beyond import substitution")
