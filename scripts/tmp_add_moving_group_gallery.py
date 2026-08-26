import json
from pathlib import Path

path = Path("web/python/examples/manim_tutorial_manifest.json")
manifest = json.loads(path.read_text(encoding="utf-8"))
entries = manifest["entries"]
if any(entry["id"] == "parity-moving-group-to-destination" for entry in entries):
    raise SystemExit("MovingGroupToDestination entry already exists")
entries.append(
    {
        "id": "parity-moving-group-to-destination",
        "title": "MovingGroupToDestination",
        "summary": "ManimCE v0.21 Example Gallery MovingGroupToDestination, byte-for-byte except for the import module.",
        "status": "ready",
        "path": "python/examples/manim_gallery_moving_group_to_destination.py",
        "category": "animations",
        "features": ["VGroup", "Dot", "animate", "get_center", "shared-family-target"],
        "upstream": "examples.html#movinggrouptodestination",
        "upstream_source": "parity/manim-v0.21/upstream-examples/moving_group_to_destination.py",
        "reuse": "source-equivalent-manim-v0.21",
        "parity_status": "candidate",
        "expected_duration": 1.5,
        "thumbnail": "thumbnails/manim/exact-source.svg",
        "thumbnail_alt": "Exact-source Manim MovingGroupToDestination",
        "thumbnail_time": 0.75,
        "order": 205,
    }
)
path.write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")
