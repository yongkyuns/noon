#!/usr/bin/env python3
"""Numerically compare the eight retained terminal PNGs; no new captures.

Requires Pillow 12.3.0. The fixed preview-panel rectangle is a coarse image ROI,
visually checked against the retained 1280x800 full-page screenshots. It encloses
the scene and excludes the metrics row; it is not an exact canvas/sample crop.
"""
import argparse
import hashlib
import json
from pathlib import Path
from PIL import Image, ImageChops


def metrics(a, b):
    if a.size != b.size:
        raise ValueError("screenshot dimensions differ")
    diff = ImageChops.difference(a, b)
    histogram = diff.histogram()
    changed = sum(sum(histogram[i * 256 + 1:(i + 1) * 256]) for i in range(4))
    maximum = max(high for low, high in diff.getextrema())
    total = sum(value * histogram[channel * 256 + value] for channel in range(4) for value in range(256))
    return {"width": a.width, "height": a.height, "channels": "RGBA8",
            "changed_channels": changed, "max_channel_error": maximum,
            "absolute_channel_error_sum": total,
            "mean_channel_error": total / (4 * a.width * a.height),
            "rgb_difference_bounds": diff.convert("RGB").getbbox(),
            "pixel_exact_to_reference": changed == 0}


def compare(root):
    provenance = json.loads((root / "provenance.json").read_text())
    frames = [entry for block in provenance["raw_captures"] for entry in block["frames"]]
    reference = Image.open(root / frames[0]["path"]).convert("RGBA")
    if reference.size != (1280, 800):
        raise ValueError("unexpected screenshot dimensions")
    preview = (597, 202, 1263, 658)
    rows = []
    for entry in frames:
        data = (root / entry["path"]).read_bytes()
        digest = hashlib.sha256(data).hexdigest()
        if digest != entry["sha256"]:
            raise ValueError("frame file hash mismatch")
        actual = Image.open(root / entry["path"]).convert("RGBA")
        rows.append({"path": entry["path"], "file_sha256": digest,
                     "full_page": metrics(reference, actual),
                     "preview_panel": metrics(reference.crop(preview), actual.crop(preview))})
    return {"schema": 2, "reference": frames[0]["path"],
            "preview_panel_rectangle_xyxy": preview,
            "preview_panel_scope": "Coarse fixed image ROI, visually checked; encloses the scene and excludes the metrics row. Not exact backing-buffer or MSAA samples.",
            "comparisons": rows,
            "all_eight_full_pages_pixel_exact": all(row["full_page"]["pixel_exact_to_reference"] for row in rows),
            "all_eight_preview_panels_pixel_exact": all(row["preview_panel"]["pixel_exact_to_reference"] for row in rows),
            "maximum_preview_channel_error": max(row["preview_panel"]["max_channel_error"] for row in rows),
            "observed_old_shader_differences": [
                {"x": 1135, "y": 393, "channel": "G", "new": 173, "old": 172},
                {"x": 1118, "y": 399, "channel": "G", "new": 123, "old": 124}],
            "observed_last_new_repeat_ui_difference": "Uploaded bytes changes from 0 B to 13.8 MiB; full-page difference bounds [1020,689,1070,697], outside preview panel.",
            "scope": "Terminal full-page PNGs only. This is not exact cross-shader pixel parity and establishes no intermediate-frame, backing-buffer, physical-scanout, or MSAA-sample equivalence. Separate renderer oracles remain independent evidence."}


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("package", type=Path)
    parser.add_argument("--output", type=Path)
    args = parser.parse_args()
    result = compare(args.package)
    encoded = json.dumps(result, indent=2) + "\n"
    if args.output:
        args.output.write_text(encoded)
    print(encoded, end="")
