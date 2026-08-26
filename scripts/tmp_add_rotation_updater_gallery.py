from pathlib import Path

manifest_path = Path("web/python/examples/manim_tutorial_manifest.json")
text = manifest_path.read_text(encoding="utf-8")

if '"id": "parity-rotation-updater"' in text:
    raise SystemExit("RotationUpdater manifest entry already exists")

marker = '''    {
      "id": "text-and-math",'''
entry = '''    {
      "id": "parity-rotation-updater",
      "title": "RotationUpdater",
      "summary": "ManimCE v0.21 Example Gallery RotationUpdater, byte-for-byte except for the import module.",
      "status": "ready",
      "path": "python/examples/manim_gallery_rotation_updater.py",
      "category": "animations",
      "features": ["Line", "add_updater", "remove_updater", "rotate_about_origin", "wait", "host-callback-lifecycle"],
      "upstream": "examples.html",
      "upstream_source": "parity/manim-v0.21/upstream-examples/rotation_updater.py",
      "reuse": "source-equivalent-manim-v0.21",
      "parity_status": "candidate",
      "expected_duration": 4.5,
      "thumbnail": "thumbnails/manim/exact-source.svg",
      "thumbnail_alt": "Exact-source Manim RotationUpdater",
      "thumbnail_time": 2.0,
      "order": 210
    },
'''

if marker not in text:
    raise SystemExit("manifest insertion marker not found")

manifest_path.write_text(text.replace(marker, entry + marker, 1), encoding="utf-8")

upstream = Path("parity/manim-v0.21/upstream-examples/rotation_updater.py").read_text(
    encoding="utf-8"
)
public = Path("web/python/examples/manim_gallery_rotation_updater.py").read_text(
    encoding="utf-8"
)
if upstream.count("from manim import *") != 1:
    raise SystemExit("upstream fixture must contain exactly one Manim star import")
expected = upstream.replace("from manim import *", "from noon import *", 1)
if public != expected:
    raise SystemExit("Noon gallery source differs from upstream beyond import substitution")
