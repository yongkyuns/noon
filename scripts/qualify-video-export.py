#!/usr/bin/env python3
"""Independently inspect and decode native export artifacts; no engine state is used."""
from __future__ import annotations

import argparse
from fractions import Fraction
import json
from pathlib import Path
import subprocess


def command(args: list[str], *, timeout: int = 120) -> bytes:
    result = subprocess.run(args, capture_output=True, timeout=timeout, check=False)
    if result.returncode:
        raise RuntimeError(f"{args[0]} exited {result.returncode}: {result.stderr.decode(errors='replace')}")
    return result.stdout


def qualify(root: Path, case: dict) -> dict:
    name = case["name"]
    if not isinstance(name, str) or Path(name).name != name:
        raise ValueError("test case name must be a simple filename stem")
    path = root / f"{name}.mp4"
    width, height, count = (int(case[k]) for k in ("width", "height", "frames"))
    rate = Fraction(case["fps"])
    step = 1 / rate
    metadata = json.loads(command([
        "ffprobe", "-v", "error", "-select_streams", "v:0", "-count_frames",
        "-show_streams", "-show_frames", "-show_format", "-of", "json", str(path),
    ]))
    assert len(metadata["streams"]) == 1, metadata["streams"]
    stream = metadata["streams"][0]
    assert stream["codec_name"] == "h264", stream
    assert stream["pix_fmt"] == "yuv420p", stream
    assert (stream["width"], stream["height"]) == (width, height), stream
    assert Fraction(stream["avg_frame_rate"]) == rate, stream
    assert int(stream["nb_read_frames"]) == count, stream
    expected_color = {"color_range": "tv", "color_space": "bt709", "color_transfer": "iec61966-2-1", "color_primaries": "bt709"}
    assert all(stream.get(k) == v for k, v in expected_color.items()), stream
    tb = Fraction(stream["time_base"])
    assert int(stream["start_pts"]) * tb == 0, stream
    assert int(stream["duration_ts"]) * tb == count * step, stream
    decoded_frames = metadata["frames"]
    assert len(decoded_frames) == count, len(decoded_frames)
    for index, frame in enumerate(decoded_frames):
        assert int(frame["pts"]) * tb == index * step, (index, frame, tb)
        duration = frame.get("duration", frame.get("pkt_duration"))
        assert duration is not None and int(duration) * tb == step, (index, frame, tb)
    # Container duration is printed to six decimals. It is not the exact clock oracle.
    assert abs(Fraction(metadata["format"]["duration"]) - count * step) <= Fraction(1, 1_000_000)

    reference = (root / f"{name}.rgba").read_bytes()
    frame_bytes = width * height * 4
    assert len(reference) == count * frame_bytes, len(reference)
    # Explicit inverse range/matrix conversion, no FPS override/filter and no
    # transfer-curve change. Input frames are compared in presentation order.
    decoded = command([
        "ffmpeg", "-v", "error", "-nostdin", "-i", str(path), "-map", "0:v:0",
        "-vf", "scale=in_range=limited:out_range=full:in_color_matrix=bt709:flags=accurate_rnd+full_chroma_int,format=rgba",
        "-fps_mode", "passthrough", "-f", "rawvideo", "pipe:1",
    ])
    assert len(decoded) == len(reference), (len(decoded), len(reference))
    errors = []
    for index in range(count):
        start = index * frame_bytes
        left = reference[start:start + frame_bytes]
        right = decoded[start:start + frame_bytes]
        assert all(value == 255 for value in right[3::4]), index
        # RGB only. This is a fixed, declared lossy threshold, not byte parity.
        total = sum(abs(a - b) for channel in range(3) for a, b in zip(left[channel::4], right[channel::4]))
        mae = total / (width * height * 3)
        assert mae <= float(case["max_mae"]), (name, index, mae, case["max_mae"])
        errors.append(mae)
    report = {
        "name": name, "width": width, "height": height, "frames": count,
        "fps": str(rate), "time_base": str(tb), "duration_exact": str(count * step),
        "first_pts": decoded_frames[0]["pts"], "last_pts": decoded_frames[-1]["pts"],
        "max_frame_rgb_mae": max(errors), "mean_frame_rgb_mae": sum(errors) / count,
        "color": expected_color, "bytes": path.stat().st_size,
    }
    (root / f"{name}.ffprobe.json").write_text(json.dumps(metadata, indent=2) + "\n")
    return report


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("directory", type=Path)
    parser.add_argument("--remove-reference", action="store_true")
    args = parser.parse_args()
    cases = sorted(args.directory.glob("*.case.json"))
    if not cases:
        raise RuntimeError("no native export qualification cases were produced")
    reports = [qualify(args.directory, json.loads(case.read_text())) for case in cases]
    result = {"ffmpeg": command(["ffmpeg", "-version"]).decode().splitlines()[0],
              "ffprobe": command(["ffprobe", "-version"]).decode().splitlines()[0], "cases": reports}
    text = json.dumps(result, indent=2) + "\n"
    (args.directory / "verification.json").write_text(text)
    print(text, end="")
    if args.remove_reference:
        for report in reports:
            (args.directory / f"{report['name']}.rgba").unlink()


if __name__ == "__main__":
    main()
