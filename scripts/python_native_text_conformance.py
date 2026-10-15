#!/usr/bin/env python3
"""Decode every native Text frame against the independent compiled Rust source."""
import argparse
import asyncio
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]
p = argparse.ArgumentParser(description=__doc__)
p.add_argument("--python-path", type=Path, required=True)
p.add_argument("--rust-binary", type=Path, required=True)
p.add_argument("--output", type=Path, required=True)
a = p.parse_args()
sys.path.insert(0, str(a.python_path.resolve()))
from noon_native import export_source


def require(condition, message):
    if not condition:
        raise RuntimeError(message)


def run(command):
    result = subprocess.run(command, capture_output=True, timeout=120)
    require(result.returncode == 0, result.stderr.decode(errors="replace"))
    return result.stdout


def decode(bundle):
    frames = sorted((bundle / "frames").glob("frame-*.png"))
    require([x.name for x in frames] == [f"frame-{i:010d}.png" for i in range(6)],
            "text frame numbering/count changed")
    raw = run(["ffmpeg", "-v", "error", "-i", str(bundle / "frames/frame-%010d.png"),
               "-vsync", "0", "-f", "rawvideo", "-pix_fmt", "rgba", "pipe:1"])
    require(len(raw) == 6 * 320 * 180 * 4, "decoded dimensions changed")
    require(any(raw[i] for i in range(len(raw)) if i % 4 != 3), "text output is blank")
    return raw


async def main():
    out = a.output.resolve()
    out.mkdir(parents=True, exist_ok=False)
    flag = os.environ.get("NOON_EXPORT_FORCE_FALLBACK", "1")
    require(flag in ("0", "1"), "fallback flag must be 0 or 1")
    source_file = ROOT / "parity/python-host/native_text.py"
    source = source_file.read_text()
    rust = out / "rust"
    run([str(a.rust_binary.resolve()), str(rust)])
    expected = decode(rust)
    (out / "rust.rgba").write_bytes(expected)
    summaries = []
    for portable in (False, True):
        target = out / ("portable" if portable else "ordinary")
        summary = await export_source(source, target, portable=portable, filename=str(source_file),
                                      width=320, height=180, fps=(30,1), png=True,
                                      max_frames=16, fallback=flag == "1")
        require(summary["frames"] == 6, "native text frame policy changed")
        actual = decode(target)
        (out / (target.name + ".rgba")).write_bytes(actual)
        require(actual == expected, f"{target.name}: Rust/Python text pixels differ")
        summaries.append({"portable": portable, "summary": summary,
                          "rgba_sha256": hashlib.sha256(actual).hexdigest()})
    (out / "report.json").write_text(json.dumps({
        "source_sha256": hashlib.sha256(source.encode()).hexdigest(),
        "all_frames_equal": True, "frames_per_route": 6, "results": summaries,
    }, indent=2) + "\n")
    print("Native plain/markup Text: every Rust/Python PNG pixel matches on both source routes")


asyncio.run(main())
