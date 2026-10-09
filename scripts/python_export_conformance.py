#!/usr/bin/env python3
"""Real native CPython export against the compiled Rust counterpart and FFmpeg.

No browser, mock renderer or Python frame evaluator participates in this proof.
"""
from __future__ import annotations
import asyncio
import gc
import hashlib
import json
from pathlib import Path
import subprocess
import sys
import time

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "build/python"))
from noon import Scene, Square, BLUE, RIGHT, UP
from noon_native import export_scene, export_source
import _manim_updaters


def require(condition, message):
    if not condition:
        raise RuntimeError(message)


def run(args):
    result = subprocess.run(args, stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=120)
    require(result.returncode == 0, result.stderr.decode(errors="replace"))
    return result.stdout


def png_pixels(bundle, width=65, height=33):
    files = sorted((bundle / "frames").glob("frame-*.png"))
    require(files, f"no images in {bundle}")
    require([p.name for p in files] == [f"frame-{i:010d}.png" for i in range(len(files))],
            "PNG numbering is not complete and sequential")
    pixels = run(["ffmpeg", "-v", "error", "-start_number", "0", "-i",
                  str(bundle / "frames/frame-%010d.png"), "-vsync", "0",
                  "-f", "rawvideo", "-pix_fmt", "rgba", "pipe:1"])
    size = width * height * 4
    require(len(pixels) == len(files) * size, "decoded PNG dimensions/count changed")
    return [pixels[i:i + size] for i in range(0, len(pixels), size)]


class Static(Scene):
    async def construct(self):
        marker = Square(side_length=0.75).set_fill(BLUE, opacity=1).set_stroke(width=0)
        # Explicit RGB makes the Rust/Python oracle independent of named palettes.
        marker.set_fill("#0000ff", opacity=1)
        marker.move_to([-2, 0, 0])
        self.add(marker)
        await self.wait(0.105)
        marker.move_to([0.5, 1, 0])
        await self.wait(0.207)
        marker.move_to([2, -1, 0])


async def main():
    output = ROOT / "artifacts/python-export"
    output.mkdir(parents=True, exist_ok=False)
    before = (len(_manim_updaters._CANONICAL_SESSIONS), len(_manim_updaters._TRACKED_MOBJECTS))
    reports = []
    common = dict(width=65, height=33, max_frames=64, fallback=True, png=True)
    binary = ROOT / "target/release/examples/python_export_contract"
    for name, p, q, start, hold, expected in [
        ("full", 30, 1, 0, 0, 10), ("crop", 30, 1, 4, 0, 6),
        ("hold", 30, 1, 0, 0.1, 13), ("fractional", 60000, 1001, 0, 0, 19),
    ]:
        native = output / (name + "-python")
        rust = output / (name + "-rust")
        summary = await export_scene(Static, native, fps=(p, q), start_frame=start,
                                     final_hold=hold, **common)
        require(summary["frames"] == expected and summary["fps"] == (p, q), "Python frame grid changed")
        run([str(binary), str(rust), str(p), str(q), str(start), str(hold)])
        a, b = png_pixels(native), png_pixels(rust)
        require(len(a) == expected and a == b, f"{name}: Rust/Python capture mismatch")
        reports.append(dict(case=name, frames=expected, pixels_sha256=hashlib.sha256(b"".join(a)).hexdigest()))

    traces = []
    callback_frames = []
    for name, start, hold, delay in [("full", 0, 0, 0), ("crop", 4, 0, 0.001), ("hold", 0, 0.1, 0.001)]:
        calls = []
        class Callbacks(Scene):
            async def construct(self):
                marker = Square(side_length=0.75).set_fill("#0000ff", opacity=1).set_stroke(width=0)
                self.add(marker)
                def first(m, dt):
                    time.sleep(delay)
                    calls.append(("first", dt, tuple(m.get_center())))
                    m.shift((dt * dt + 0.01) * RIGHT)
                def second(m, dt):
                    calls.append(("second", dt, tuple(m.get_center())))
                    m.shift(abs(m.get_center()[0]) * 0.02 * UP)
                marker.add_updater(first)
                marker.add_updater(second)
                await self.wait(0.105)
                marker.move_to([0.5, 1, 0])
                await self.wait(0.207)
                marker.move_to([2, -1, 0])
        target = output / ("callbacks-" + name)
        await export_scene(Callbacks, target, start_frame=start, final_hold=hold, fps=(30, 1), **common)
        traces.append(calls)
        callback_frames.append(png_pixels(target))
    require(traces[0] and traces[0] == traces[1] == traces[2], "crop/hold/delay changed callback history")
    require(callback_frames[1] == callback_frames[0][4:], "callback crop pixels changed")
    require(callback_frames[2][:10] == callback_frames[0], "hold changed prior frames")
    require(len(callback_frames[2]) == 13 and len(set(callback_frames[2][10:])) == 1,
            "terminal hold changed pixels or frame count")

    for p, q, expected in [(30, 1, 10), (30000, 1001, 10), (60000, 1001, 19)]:
        movie = output / f"python-{p}-{q}.mp4"
        summary = await export_scene(Static, movie, width=64, height=32, fps=(p, q), max_frames=64, fallback=True)
        probe = json.loads(run(["ffprobe", "-v", "error", "-show_streams", "-show_frames", "-of", "json", str(movie)]))
        require(summary["frames"] == expected, "MP4 source count changed")
        require(probe["streams"][0]["time_base"] == f"1/{p}", "MP4 time base changed")
        require([int(f["pts"]) for f in probe["frames"]] == [i * q for i in range(expected)], "MP4 PTS drift")
        raw = run(["ffmpeg", "-v", "error", "-i", str(movie), "-vsync", "0", "-f", "rawvideo", "-pix_fmt", "rgba", "pipe:1"])
        require(len(raw) == expected * 64 * 32 * 4, "MP4 decoded count changed")

    class Broken(Scene):
        async def construct(self):
            self.add(Square())
            await self.wait(0.105)
            raise ValueError("intentional source failure")
    old = output / "preserve.mp4"
    old.write_bytes(b"preserve-existing-output")
    try:
        await export_scene(Broken, old, width=64, height=32, overwrite=True, max_frames=64, fallback=True)
    except ValueError as error:
        require(str(error) == "intentional source failure", "original source failure was replaced")
    else:
        raise RuntimeError("broken source exported successfully")
    require(old.read_bytes() == b"preserve-existing-output", "failed export replaced old file")

    # A normal synchronous source must use the same optional driver, not a shim.
    source = "from noon import *\nresult = Scene()\nresult.add(Square())\nresult.wait(0.1)\n"
    summary = await export_source(source, output / "sync-source", fps=(30, 1), **common)
    require(summary["frames"] == 3, "synchronous source did not use the export frame grid")
    try:
        await export_scene(Static, output / "cap.mp4", width=64, height=32, max_frames=1, fallback=True)
    except RuntimeError:
        pass
    else:
        raise RuntimeError("frame safety cap silently truncated a source")
    require(not (output / "cap.mp4").exists(), "capped export published a partial file")
    gc.collect()
    require(before == (len(_manim_updaters._CANONICAL_SESSIONS), len(_manim_updaters._TRACKED_MOBJECTS)),
            "native export leaked callback handles")
    (output / "report.json").write_text(json.dumps(reports, indent=2))
    print("Native Python export: Rust PNG parity, callbacks, fractional MP4 PTS, sync source and failure cleanup passed")


if __name__ == "__main__":
    asyncio.run(main())
