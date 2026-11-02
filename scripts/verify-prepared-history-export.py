#!/usr/bin/env python3
"""Decode the original ordinary Python export corpus without importing Noon.

The source math oracle uses the preserved Fourier preparation helpers and model
capture. Audio uses the unmixed native composition, independently resampled and
enveloped here. Neither export receipts nor encoded file size prove media.
"""
from __future__ import annotations

import argparse
from fractions import Fraction
import hashlib
import importlib.util
import json
from pathlib import Path
import subprocess
import sys
import wave

import numpy as np


ROOT = Path(__file__).resolve().parents[1]
WIDTH, HEIGHT, P, Q, RATE = 640, 360, 30_000, 1_001, 48_000
FRAME_BYTES = WIDTH * HEIGHT * 4
MINIMAL_SHA = "a898089e0534e31a5a149daaff4fec95ffc7ba5d108125d8ba93b4e252e5a60b"
FOURIER_SHA = "7a09cda6039efdc9901a3241b0c514f634996187a0ea9b1f316e34dbbd1feed9"
COMBINED_SHA = "9a2ba12dde3ad3941cdde00ba9eaa3c0cde82ec2869a85b6dda38aeb3b19c6ff"
MODEL_SHA = "5ee5435db5f8b551228c272a421f2161913d91edcb720e24bd469315a43c40df"
CASES = {
    "minimal": {"start": 0, "frames": 90, "end": Fraction(3), "ticks": 90_000,
                "pcm": 144_144, "presented": 144_000, "offset": 0,
                "fade_in": .5, "fade_out": .5, "source_sha": MINIMAL_SHA},
    "fourier-full": {"start": 0, "frames": 1039, "end": Fraction(693, 20), "ticks": 1_039_500,
                     "pcm": 1_664_063, "presented": 1_663_200, "offset": 0,
                     "fade_in": 1.5, "fade_out": 2.5, "source_sha": COMBINED_SHA},
    "fourier-crop": {"start": 700, "frames": 339, "end": Fraction(693, 20), "ticks": 338_800,
                     "pcm": 542_943, "presented": 542_080, "offset": 1_121_120,
                     "fade_in": 1.5, "fade_out": 2.5, "source_sha": COMBINED_SHA},
}


class VerificationError(RuntimeError):
    """A decoded artifact violates an independently pinned source contract."""


def require(condition: bool, description: object) -> None:
    # Deliberately remains active under Python -O and -OO.
    if not condition:
        raise VerificationError(str(description))


def digest(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def file_digest(filename: Path) -> str:
    with filename.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def write_json(filename: Path, value: object) -> None:
    filename.write_text(json.dumps(value, indent=2, allow_nan=False) + "\n")


def command(args: list[str], timeout: int = 180) -> bytes:
    result = subprocess.run(args, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                            timeout=timeout, check=False)
    require(result.returncode == 0,
            f"{args[0]} failed ({result.returncode}): {result.stderr.decode(errors='replace')}")
    return result.stdout


def probe_movie(movie: Path) -> dict:
    video = json.loads(command(["ffprobe", "-v", "error", "-select_streams", "v:0", "-count_frames",
        "-show_streams", "-show_frames", "-show_format", "-of", "json", str(movie)]))
    audio = json.loads(command(["ffprobe", "-v", "error", "-select_streams", "a:0",
        "-show_streams", "-show_packets", "-of", "json", str(movie)]))
    return {"video": video, "audio": audio}


def verify_timing(probe: dict, case: dict) -> dict:
    require(len(probe["video"]["streams"]) == len(probe["audio"]["streams"]) == 1, "one video and one soundtrack")
    video, audio = probe["video"]["streams"][0], probe["audio"]["streams"][0]
    frames, ticks = case["frames"], case["ticks"]
    require((video["codec_name"], video["width"], video["height"], video["pix_fmt"]) ==
            ("h264", WIDTH, HEIGHT, "yuv420p"), "H.264 dimensions or pixel format changed")
    require(Fraction(video["r_frame_rate"]) == Fraction(P, Q), "nominal frame rate changed")
    require(Fraction(video["avg_frame_rate"]) == Fraction(frames * P, ticks), "average rate must retain the partial terminal interval")
    require(Fraction(video["time_base"]) == Fraction(1, P), "video time base changed")
    require(int(video["nb_read_frames"]) == frames and int(video["start_pts"]) == 0, "video count or rebased origin changed")
    require(int(video["duration_ts"]) == ticks, "video presentation endpoint changed")
    rows = probe["video"]["frames"]
    require([int(row["pts"]) for row in rows] == [i * Q for i in range(frames)], "video PTS sequence changed")
    require([int(row.get("duration", row.get("pkt_duration", -1))) for row in rows] ==
            [min(Q, ticks - i * Q) for i in range(frames)], "video sample duration changed")
    for key, value in {"color_range": "tv", "color_space": "bt709", "color_primaries": "bt709",
                       "color_transfer": "iec61966-2-1"}.items():
        require(video[key] == value, f"{key} changed")
    require(abs(float(probe["video"]["format"]["duration"]) - ticks / P) <= case.get("container_tolerance", 1e-6),
            "container endpoint changed")
    require((audio["codec_name"], int(audio["sample_rate"]), int(audio["channels"])) ==
            ("aac", RATE, 2), "AAC stereo format changed")
    require(Fraction(audio["time_base"]) == Fraction(1, RATE), "audio time base changed")
    require(int(audio["start_pts"]) == 0 and int(audio["duration_ts"]) == case["presented"],
            "audio edit/presentation interval changed")
    packets = probe["audio"]["packets"]
    require(packets and int(packets[0]["pts"]) == -1024 and int(packets[0]["duration"]) == 1024,
            "AAC priming packet changed")
    require(any(int(side.get("skip_samples", 0)) == 1024 for side in packets[0].get("side_data_list", [])),
            "AAC priming must be explicitly skipped")
    times = [int(packet["pts"]) for packet in packets]
    require(all(b > a for a, b in zip(times, times[1:])), "AAC packet PTS are not strictly ordered")
    require(max(int(packet["pts"]) + int(packet["duration"]) for packet in packets) == case["presented"],
            "AAC packet presentation endpoint changed")
    return {"frames": frames, "time_base": f"1/{P}", "fps": f"{P}/{Q}", "duration_ticks": ticks,
            "last_duration_ticks": ticks - (frames - 1) * Q, "presented_audio_frames": case["presented"],
            "aac_priming_frames": 1024}


def source_oracle() -> dict:
    fixtures = ROOT / "tests/fixtures/fourier_audio"
    for name, expected in {"audio_ordinary_play.py": MINIMAL_SHA, "fourier_epicycles.py": FOURIER_SHA,
                           "fourier_model_capture.json": MODEL_SHA}.items():
        require(file_digest(fixtures / name) == expected, f"preserved {name} changed")
    spec = importlib.util.spec_from_file_location("fourier_math_oracle", ROOT / "scripts/sampled-motion-fixture-check.py")
    require(spec is not None and spec.loader is not None, "missing preserved source math checker")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    oracle = module.check(True)
    require([stage["terms"] for stage in oracle["stages"]] == [7, 21, 61], "Fourier stages changed")
    require(oracle["original_sha256"] == FOURIER_SHA and oracle["model_capture_sha256"] == MODEL_SHA,
            "source math oracle identities changed")
    return oracle


def stage_samples(oracle: dict) -> dict[int, tuple[dict, float]]:
    selected = {}
    for stage in oracle["stages"]:
        start = stage["oracle"]["draw_start_seconds"]
        targets = [start + phase * 9 for phase in (.07, .31, .62, .91)] + [start + 9.2]
        indices = {round(time * P / Q) for time in targets}
        # Grid samples on both sides of each draw boundary are selected using
        # source time, never the generated capture's reported sample positions.
        indices.update((int(np.ceil(start * P / Q)), int(np.floor((start + 9) * P / Q))))
        for index in sorted(indices):
            phase = min(1., max(0., (index * Q / P - start) / 9))
            selected[index] = (stage, phase)
    return selected


def interpolate_endpoint(stage: dict, phases: np.ndarray) -> np.ndarray:
    oracle = stage["oracle"]
    keys = np.asarray(oracle["keys"])
    endpoint = np.asarray(oracle["chain"])[:, -1, :]
    return np.stack([np.interp(phases, keys, endpoint[:, axis]) for axis in (0, 1)], axis=-1)


def pixels_for(points: np.ndarray) -> np.ndarray:
    return np.stack([WIDTH / 2 + points[..., 0] * HEIGHT / 8,
                     HEIGHT / 2 - points[..., 1] * HEIGHT / 8], axis=-1)


def nearest_mask(mask: np.ndarray, point: np.ndarray, radius: float = 4.) -> float:
    x, y = point
    left, right = max(0, int(x - radius)), min(WIDTH, int(x + radius) + 2)
    top, bottom = max(0, int(y - radius)), min(HEIGHT, int(y + radius) + 2)
    yy, xx = np.nonzero(mask[top:bottom, left:right])
    if not len(xx):
        return float("inf")
    return float(np.min(np.hypot(xx + left + .5 - x, yy + top + .5 - y)))


def verify_fourier_picture(rgba: np.ndarray, stage: dict, phase: float, label: str, decoded: bool) -> dict:
    rgb = rgba[..., :3].astype(np.int16)
    red, green, blue = (rgb[..., i] for i in range(3))
    yellow = (red > 170) & (green > 140) & (red - blue > 55) & (green - blue > 25)
    teal = (green > 70) & (green - red > 25) & (blue - red > 20) & (green - blue > 5)
    tip = pixels_for(interpolate_endpoint(stage, np.asarray([phase])))[0]
    tip_error = nearest_mask(yellow, tip)
    require(tip_error <= (2.7 if decoded else 2.3), f"{label}: {stage['terms']}-term pen is missing at its independent source position")
    x, y = tip
    yy, xx = np.ogrid[:HEIGHT, :WIDTH]
    tip_pixels = int(np.count_nonzero(yellow & ((xx + .5 - x) ** 2 + (yy + .5 - y) ** 2 <= 3.1 ** 2)))
    require(tip_pixels >= 5, f"{label}: yellow disk is missing at the pen")
    distances = []
    if phase > .02:
        phases = np.linspace(.04 * phase, .96 * phase, 16)
        points = pixels_for(interpolate_endpoint(stage, phases))
        distances = [nearest_mask(teal, point) for point in points]
        require(max(distances) <= (3.5 if decoded else 3.), f"{label}: revealed trace diverges from the source chords: {distances}")
    future_probes = 0
    if phase < .85:
        # Avoid portions that geometrically revisit the already drawn prefix.
        # The remaining probes reject a prematurely complete trace, which a
        # positive-only check of the existing prefix could otherwise accept.
        past = interpolate_endpoint(stage, np.linspace(0., phase, 257))
        future = interpolate_endpoint(stage, np.linspace(phase + .08, .99, 12))
        separation = np.min(np.linalg.norm(future[:, None, :] - past[None, :, :], axis=2), axis=1)
        future = pixels_for(future[separation > .16])
        strong_teal = (green > 100) & (green - red > 55) & (blue - red > 35) & (green - blue > 10)
        for point in future:
            require(nearest_mask(strong_teal, point) > 2., f"{label}: future trace was revealed before its source phase")
        future_probes = len(future)
        require(future_probes > 0, f"{label}: source oracle has no distinct unrevealed trace probe")
    return {"terms": stage["terms"], "phase": phase, "tip_scene": interpolate_endpoint(stage, np.asarray([phase]))[0].tolist(),
            "tip_pixel_error": tip_error, "tip_pixels": tip_pixels,
            "max_trace_pixel_error": max(distances, default=0.), "unrevealed_trace_probes": future_probes}


def verify_fourier_terminal_picture(rgba: np.ndarray, stage: dict, label: str, decoded: bool) -> dict:
    """Observe the original source's final rig removal and caption replacement."""
    require(stage["terms"] == 61, f"{label}: terminal trace must use the final 61-term stage")
    rgb = rgba[..., :3].astype(np.int16)
    red, green, blue = (rgb[..., i] for i in range(3))
    yellow = (red > 170) & (green > 140) & (red - blue > 55) & (green - blue > 25)
    remaining_rig_pixels = int(np.count_nonzero(yellow))
    require(remaining_rig_pixels == 0, f"{label}: final picture still contains the yellow rig vectors or pen")
    teal = (green > 70) & (green - red > 25) & (blue - red > 20) & (green - blue > 5)
    points = pixels_for(interpolate_endpoint(stage, np.linspace(.04, .96, 16)))
    distances = [nearest_mask(teal, point) for point in points]
    require(max(distances) <= (3.5 if decoded else 3.),
            f"{label}: final picture lost the completed 61-term source trace: {distances}")

    # The fixed source replaces its gray caption with teal finished text at
    # (0, -2.8), above the separate term labels at y=-3.35. Inspect that band,
    # rather than accepting teal trace/label pixels elsewhere as the caption.
    center_x, center_y = pixels_for(np.asarray([[0., -2.8]]))[0]
    top, bottom = int(center_y - 10), int(center_y + 10) + 1
    yy, xx = np.nonzero(teal[top:bottom])
    require(len(xx) >= 20 and np.ptp(yy) >= 2 and
            int(xx.min()) < center_x - 40 and int(xx.max()) > center_x + 40,
            f"{label}: final teal finished caption is missing from its source position")
    return {"terms": 61, "remaining_yellow_rig_pixels": remaining_rig_pixels,
            "completed_trace_probes": len(points), "max_trace_pixel_error": max(distances),
            "finished_caption_teal_pixels": len(xx),
            "finished_caption_bounds": [int(xx.min()), int(yy.min()) + top,
                                        int(xx.max()), int(yy.max()) + top]}


def verify_circle(rgba: np.ndarray, label: str) -> dict:
    active = np.max(rgba[..., :3], axis=2) > 50
    yy, xx = np.nonzero(active)
    require(len(xx) > 100, f"{label}: completed ordinary Circle is missing")
    bounds = [int(xx.min()), int(yy.min()), int(xx.max()), int(yy.max())]
    center = [float(np.mean(xx + .5)), float(np.mean(yy + .5))]
    require(abs(center[0] - WIDTH / 2) <= 1.2 and abs(center[1] - HEIGHT / 2) <= 1.2,
            f"{label}: ordinary Circle center changed")
    require(all(abs(value - expected) <= 3 for value, expected in zip(bounds,
            [WIDTH / 2 - 22.5, HEIGHT / 2 - 22.5, WIDTH / 2 + 22.5, HEIGHT / 2 + 22.5])),
            f"{label}: ordinary Circle radius changed: {bounds}")
    return {"bounds": bounds, "center": center, "active_pixels": len(xx)}


def decoded_frames(movie: Path, stderr_path: Path):
    args = ["ffmpeg", "-v", "error", "-nostdin", "-i", str(movie), "-map", "0:v:0", "-vf",
            "scale=in_range=limited:out_range=full:in_color_matrix=bt709,format=rgba",
            "-fps_mode", "passthrough", "-f", "rawvideo", "pipe:1"]
    with stderr_path.open("wb") as errors:
        process = subprocess.Popen(args, stdout=subprocess.PIPE, stderr=errors)
        try:
            require(process.stdout is not None, "decoder pipe unavailable")
            while True:
                data = process.stdout.read(FRAME_BYTES)
                if not data:
                    break
                require(len(data) == FRAME_BYTES, "decoder produced an incomplete RGBA frame")
                yield data
            require(process.wait(timeout=60) == 0, f"video decoder failed; inspect {stderr_path}")
        finally:
            if process.poll() is None:
                process.terminate()
                try:
                    process.wait(timeout=10)
                except subprocess.TimeoutExpired:
                    process.kill()
                    process.wait(timeout=10)
            if process.stdout is not None:
                process.stdout.close()


def verify_pixels(movie: Path, capture: Path, case: dict, oracle: dict | None, output: Path) -> dict:
    reference = capture / "frames.rgba"
    require(reference.stat().st_size == case["frames"] * FRAME_BYTES, "raw video length changed")
    visual = case.get("visual", "minimal" if movie.stem == "minimal" else "fourier")
    selected = stage_samples(oracle) if visual == "fourier" else {}
    rows, checks, circle, terminal = [], [], None, None
    with reference.open("rb") as stream:
        for index, data in enumerate(decoded_frames(movie, output / f"{movie.stem}.video-decode.log")):
            require(index < case["frames"], "extra decoded video frame")
            expected_bytes = stream.read(FRAME_BYTES)
            expected = np.frombuffer(expected_bytes, dtype=np.uint8).reshape(HEIGHT, WIDTH, 4)
            actual = np.frombuffer(data, dtype=np.uint8).reshape(HEIGHT, WIDTH, 4)
            difference = np.abs(expected.astype(np.int16) - actual.astype(np.int16))
            mean = float(np.mean(difference))
            active = np.max(expected[..., :3], axis=2) > 12
            foreground = float(np.mean(difference[..., :3][active])) if np.any(active) else 0.
            require(mean <= 3., f"{movie.stem}:{index}: RGBA mean error {mean}")
            require(foreground <= 25., f"{movie.stem}:{index}: foreground RGB error {foreground}")
            source_index = index + case["start"]
            if visual == "fourier" and source_index in selected:
                stage, phase = selected[source_index]
                checks.append({"source_frame": source_index, "source_time": source_index * Q / P,
                    "captured": verify_fourier_picture(expected, stage, phase, f"captured:{source_index}", False),
                    "decoded": verify_fourier_picture(actual, stage, phase, f"decoded:{source_index}", True)})
            if visual == "fourier" and source_index == 1038:
                require(index == case["frames"] - 1, "terminal source picture must be the last encoded frame")
                stage = oracle["stages"][-1]
                terminal = {"source_frame": source_index, "source_time": source_index * Q / P,
                    "captured": verify_fourier_terminal_picture(expected, stage, f"captured:{source_index}", False),
                    "decoded": verify_fourier_terminal_picture(actual, stage, f"decoded:{source_index}", True)}
            if visual == "minimal" and index == 30:
                circle = {"captured": verify_circle(expected, "captured"), "decoded": verify_circle(actual, "decoded")}
            rows.append({"frame": index, "source_frame": source_index, "rgba_mean_error": mean,
                         "foreground_rgb_mean_error": foreground, "capture_sha256": digest(expected_bytes),
                         "decoded_sha256": digest(data),
                         "capture_bright_pixels": int(np.count_nonzero(np.max(expected[..., :3], axis=2) > 50))})
    require(len(rows) == case["frames"], "missing decoded frames")
    if visual == "immutable":
        require(len({row["capture_sha256"] for row in rows}) == 1 and rows[0]["capture_bright_pixels"] > 1000,
                "the existing lesson's initial one-second declared wait must retain its visible static picture")
    else:
        require(len({row["capture_sha256"] for row in rows}) > 10, "a frozen capture cannot prove ordinary animation")
    if visual == "minimal":
        require(circle is not None, "completed circle observation missing")
        require(len({row["capture_sha256"] for row in rows[30:]}) == 1, "ordinary trailing wait must retain the same picture")
        require(rows[0]["capture_bright_pixels"] <= 1, "ordinary Create begins with no drawn circle")
        complete = rows[30]["capture_bright_pixels"]
        for index, lower, upper in [(7, .02, .45), (15, .3, .7), (22, .55, 1.02)]:
            fraction = rows[index]["capture_bright_pixels"] / complete
            require(lower < fraction < upper, f"ordinary Create must grow a partial stroke at frame {index}: {fraction}")
    elif visual == "fourier":
        required = {61} if case["start"] else {7, 21, 61}
        require({row["captured"]["terms"] for row in checks} == required, "missing Fourier stage oracle checks")
        expected_checks = sum(case["start"] <= index < case["start"] + case["frames"] for index in selected)
        require(len(checks) == expected_checks and len(checks) >= (5 if case["start"] else 21),
                "missing intermediate and boundary pictures")
        require(terminal is not None, "missing independent final Fourier picture check")
    return {"decoded_all_frames": True, "max_rgba_mean_error": max(row["rgba_mean_error"] for row in rows),
            "max_foreground_rgb_mean_error": max(row["foreground_rgb_mean_error"] for row in rows),
            "raw_sha256": file_digest(reference), "frames": rows, "fourier_source_checks": checks,
            "fourier_terminal_picture": terminal, "circle": circle}


def load_unmixed(wav: Path) -> np.ndarray:
    with wave.open(str(wav), "rb") as stream:
        require((stream.getnchannels(), stream.getsampwidth(), stream.getframerate(), stream.getnframes()) ==
                (2, 2, 24_000, 80 * 24_000), "unmixed original composition WAV changed")
        # The existing native generator rounds sample * i16::MAX, not 32768.
        return np.frombuffer(stream.readframes(stream.getnframes()), dtype="<i2").reshape(-1, 2).astype(np.float64) / 32767.


def expected_pcm(unmixed: np.ndarray, case: dict) -> np.ndarray:
    frames = np.arange(case["pcm"], dtype=np.int64) + case["offset"]
    lower, fraction = frames // 2, (frames % 2)[:, None] / 2.
    source = unmixed[lower] * (1 - fraction) + unmixed[lower + 1] * fraction
    time = frames.astype(np.float64) / RATE
    envelope = np.minimum(1., np.minimum(time / case["fade_in"], (float(case["end"]) - time) / case["fade_out"]))
    envelope = np.maximum(0., envelope) * .5
    return source * envelope[:, None]


def audio_metrics(actual: np.ndarray, expected: np.ndarray, limit: float, label: str) -> list[dict]:
    require(actual.shape == expected.shape and np.isfinite(actual).all(), f"{label}: malformed/nonfinite PCM")
    results = []
    for channel in range(2):
        a, b = actual[:, channel].astype(np.float64), expected[:, channel].astype(np.float64)
        error = float(np.sqrt(np.mean((a - b) ** 2)))
        expected_power, actual_power = float(np.dot(b, b)), float(np.dot(a, a))
        require(expected_power > 0 and actual_power > 0, f"{label}: missing soundtrack")
        correlation = float(np.dot(a, b) / np.sqrt(expected_power * actual_power))
        ratio = float(np.sqrt(actual_power / expected_power))
        require(error < limit and correlation > .97 and .85 < ratio < 1.15,
                f"{label}: channel {channel} phase/gain mismatch: RMS={error}, correlation={correlation}, ratio={ratio}")
        results.append({"channel": channel, "error_rms": error, "correlation": correlation, "rms_ratio": ratio})
    return results


def verify_audio(movie: Path, capture: Path | None, case: dict, unmixed: np.ndarray) -> dict:
    expected = expected_pcm(unmixed, case)
    staged = None
    report = {}
    if capture is not None:
        pcm = capture / "audio.f32le"
        require(pcm.stat().st_size == case["pcm"] * 8, "staged PCM grid length changed")
        staged = np.fromfile(pcm, dtype="<f4").reshape(-1, 2)
        require(np.isfinite(staged).all(), "staged PCM contains nonfinite values")
        maximum = float(np.max(np.abs(staged.astype(np.float64) - expected)))
        # One source-quantization LSB is conservative for the 0.5 gain, linear
        # resampling and small native/WASM transcendental differences.
        require(maximum <= 1 / 32767., f"staged PCM differs from the unmixed source/gain/fades: {maximum}")
        mixed = case.get("mixed", case["presented"])
        require(np.all(staged[mixed:] == 0), "grid padding after the authored endpoint is audible")
        report.update({"staged_sha256": file_digest(pcm), "staged_frames": case["pcm"],
                       "max_unmixed_envelope_error": maximum, "padding_frames": case["pcm"] - mixed,
                       "padding_is_exactly_silent": True,
                       "staged_comparison": audio_metrics(staged[:case["presented"]], expected[:case["presented"]], 1e-5, "staged")})
    decoded_bytes = command(["ffmpeg", "-v", "error", "-nostdin", "-i", str(movie), "-map", "0:a:0",
                             "-c:a", "pcm_f32le", "-f", "f32le", "pipe:1"])
    require(len(decoded_bytes) % 8 == 0, "incomplete decoded stereo frame")
    decoded = np.frombuffer(decoded_bytes, dtype="<f4").reshape(-1, 2)
    require(case["presented"] <= len(decoded) < case["presented"] + 1024, "unexpected decoded AAC padding")
    target = staged if staged is not None else expected
    report.update({"decoded_sha256": digest(decoded_bytes), "decoded_frames": len(decoded),
                   "presented_frames": case["presented"],
                   "aac_comparison": audio_metrics(decoded[:case["presented"]], target[:case["presented"]], .006, "decoded AAC")})
    # Each audible fade/steady interval is checked independently so a long body
    # cannot conceal a missing envelope in the aggregate correlation.
    windows = {"fade_in": (0., case["fade_in"]),
               "steady_gain": (case["fade_in"], float(case["end"]) - case["fade_out"]),
               "fade_out": (float(case["end"]) - case["fade_out"], float(case["end"]))}
    report["envelope_windows"] = {}
    for name, (start, end) in windows.items():
        first = max(0, int(round(start * RATE)) - case["offset"])
        last = min(case["presented"], int(round(end * RATE)) - case["offset"])
        if last > first:
            report["envelope_windows"][name] = audio_metrics(decoded[first:last], expected[first:last], .006, name)
    return report


def verify_capture(capture: Path, reported: dict, case: dict) -> dict:
    manifest = json.loads((capture / "capture.json").read_text())
    require(manifest["format"] == "noon-prepared-capture-v2", "capture format changed")
    require(manifest["source"]["kind"] == "completed_history", "ordinary source bypassed completed history")
    require(manifest["source"] == reported["capture_source"], "capture and admitted source identity disagree")
    require(reported["source_sha256"] == case["source_sha"], "acceptance source bytes changed")
    require(reported["browser_errors"] == [], "browser errors were discarded")
    require((manifest["width"], manifest["height"], manifest["audio"]) == (WIDTH, HEIGHT, True), "capture dimensions/audio changed")
    require(abs(manifest["source_end"] - float(case["end"])) <= 1e-9, "completed source endpoint changed")
    options = manifest["frame_options"]
    require(options["frame_rate"] == {"numerator": P, "denominator": Q} and options["start_frame"] == case["start"],
            "capture frame grid/crop changed")
    require(options["stop"] == {"kind": "source_end"} and options["final_hold_seconds"] == 0, "natural endpoint was replaced")
    samples = manifest["samples"]
    source_frames = case["frames"] + case["start"]
    require(len(samples) == source_frames + 1, "missing discarded-prefix or completion observations")
    identity = manifest["source"]["runtime_identity"]
    require(str(identity).isdigit() and int(identity) > 0, "invalid source runtime identity")
    epochs = []
    for index, sample in enumerate(samples):
        requested = index * Q / P
        published = min(requested, float(case["end"]))
        require(abs(sample["requested_time"] - requested) <= 1e-12 and
                abs(sample["published_time"] - published) <= 1e-12,
                "publication time differs from the independent sample grid/terminal clamp")
        require(sample["runtime_identity"] == identity, "capture changed runtime owners")
        epochs.append(int(sample["publication"]["frame_epoch"]))
    require(all(b > a for a, b in zip(epochs, epochs[1:])), "publication epochs are not strictly increasing")
    return {"manifest_sha256": file_digest(capture / "capture.json"), "source": manifest["source"],
            "samples": len(samples), "source_endpoint": manifest["source_end"]}


def compare_native_circle(native: np.ndarray, python: np.ndarray, frame: int) -> dict:
    a, b = np.max(native[..., :3], axis=2) > 50, np.max(python[..., :3], axis=2) > 50
    if not a.any() or not b.any():
        require(not a.any() and not b.any(), f"native/Python reveal differs at frame {frame}")
        return {"frame": frame, "blank": True}
    def dilation(mask):
        padded = np.pad(mask, 1)
        return np.logical_or.reduce([padded[y:y + HEIGHT, x:x + WIDTH] for y in range(3) for x in range(3)])
    missed_a = float(np.mean(~dilation(b)[a]))
    missed_b = float(np.mean(~dilation(a)[b]))
    require(max(missed_a, missed_b) <= .10, f"native/Python Circle reveal geometry differs at frame {frame}")
    ay, ax = np.nonzero(a)
    by, bx = np.nonzero(b)
    distance = float(np.hypot(np.mean(ax) - np.mean(bx), np.mean(ay) - np.mean(by)))
    require(distance <= 1.2, f"native/Python Circle center differs at frame {frame}")
    return {"frame": frame, "native_outside_python_fraction": missed_a,
            "python_outside_native_fraction": missed_b, "centroid_difference_pixels": distance}


def verify_native(movie: Path, output: Path, unmixed: np.ndarray) -> dict:
    case = CASES["minimal"]
    probe = probe_movie(movie)
    write_json(output / "native-minimal.ffprobe.json", probe)
    result = {"path": str(movie), "sha256": file_digest(movie), "timing": verify_timing(probe, case),
              "audio": verify_audio(movie, None, case, unmixed), "circle_comparison": []}
    count = 0
    with (output / "minimal-capture/frames.rgba").open("rb") as reference:
        for index, data in enumerate(decoded_frames(movie, output / "native-minimal.video-decode.log")):
            require(index < case["frames"], "extra native video frame")
            expected = reference.read(FRAME_BYTES)
            require(len(expected) == FRAME_BYTES, "missing Python reference frame")
            native = np.frombuffer(data, dtype=np.uint8).reshape(HEIGHT, WIDTH, 4)
            python = np.frombuffer(expected, dtype=np.uint8).reshape(HEIGHT, WIDTH, 4)
            result["circle_comparison"].append(compare_native_circle(native, python, index))
            count += 1
    require(count == case["frames"], "missing native decoded frames")
    return result


def verify(output: Path, music_wav: Path, native_video: Path | None = None) -> dict:
    oracle = source_oracle()
    write_json(output / "fourier-source-oracle.json", oracle)
    unmixed = load_unmixed(music_wav)
    report = {"status": "running", "python": sys.version, "numpy": np.__version__,
              "source_model_sha256": MODEL_SHA, "unmixed_wav_sha256": file_digest(music_wav),
              "ffmpeg_version": command(["ffmpeg", "-version"]).decode().splitlines()[0],
              "ffprobe_version": command(["ffprobe", "-version"]).decode().splitlines()[0], "cases": []}
    for name, case in CASES.items():
        movie, capture = output / f"{name}.mp4", output / f"{name}-capture"
        captured = json.loads((output / f"{name}-report.json").read_text())
        probe = probe_movie(movie)
        write_json(output / f"{name}.ffprobe.json", probe)
        row = {"name": name, "mp4_sha256": file_digest(movie), "capture": verify_capture(capture, captured, case),
               "timing": verify_timing(probe, case), "video": verify_pixels(movie, capture, case, oracle, output),
               "audio": verify_audio(movie, capture, case, unmixed)}
        report["cases"].append(row)
        write_json(output / "verification.json", report)
        print(f"{name}: all {case['frames']} frames decoded; exact PTS/end; source geometry and PCM phase/fades checked", flush=True)
    full, crop = report["cases"][1:]
    require([row["capture_sha256"] for row in crop["video"]["frames"]] ==
            [row["capture_sha256"] for row in full["video"]["frames"]][700:], "crop changed source pictures")
    full_pcm = np.memmap(output / "fourier-full-capture/audio.f32le", dtype="u1", mode="r")
    crop_pcm = np.memmap(output / "fourier-crop-capture/audio.f32le", dtype="u1", mode="r")
    offset = CASES["fourier-crop"]["offset"] * 8
    require(np.array_equal(full_pcm[offset:offset + len(crop_pcm)], crop_pcm), "crop reset or perturbed the original PCM phase")
    report["crop"] = {"source_frame": 700, "source_pcm_offset": offset // 8,
                      "captured_frames_identical": True, "staged_pcm_byte_identical": True}
    del full_pcm, crop_pcm
    if native_video is not None:
        report["native_minimal"] = verify_native(native_video, output, unmixed)
    report["status"] = "passed"
    write_json(output / "verification.json", report)
    return report


def verify_immutable(output: Path, music_wav: Path) -> dict:
    """A three-frame regression for the already supported finite source path."""
    movie, capture = output / "immutable-three.mp4", output / "immutable-three-capture"
    reported = json.loads((output / "immutable-three-report.json").read_text())
    manifest = json.loads((capture / "capture.json").read_text())
    require(manifest["format"] == "noon-prepared-capture-v2" and manifest["source"]["kind"] == "immutable",
            "existing immutable source was routed through history")
    require(reported["capture_source"] == manifest["source"] and reported["browser_errors"] == [],
            "immutable source identity/errors changed")
    require(reported["source_sha256"] == file_digest(ROOT / "web/python/examples/background_music_lesson.py"),
            "immutable regression must use the existing source unchanged")
    require(manifest["source_end"] == 30. and reported["authored_duration"] == 30., "immutable authored duration changed")
    options = manifest["frame_options"]
    require(options["stop"] == {"kind": "frame_count", "frames": 3} and options["start_frame"] == 0 and
            options["frame_rate"] == {"numerator": P, "denominator": Q}, "immutable three-frame request changed")
    require(len(manifest["samples"]) == 4, "immutable completion sample missing")
    for index, sample in enumerate(manifest["samples"]):
        require(abs(sample["requested_time"] - index * Q / P) < 1e-12 and
                sample["requested_time"] == sample["published_time"] and
                sample["runtime_identity"] == manifest["source"]["runtime_identity"],
                "immutable source publication changed")
    probe = probe_movie(movie)
    write_json(output / "immutable-three.ffprobe.json", probe)
    audio_frames = int(probe["audio"]["streams"][0]["duration_ts"])
    # This arbitrary short crop is off the PCM grid. Preserve the existing
    # finite path's at-most-one-sample audio and millisecond movie-header bound;
    # the natural 3s/34.65s acceptance cases above require exact endpoints.
    require(audio_frames in (4804, 4805), "immutable soundtrack end differs by more than one PCM sample")
    case = {"start": 0, "frames": 3, "end": Fraction(30), "ticks": 3003, "pcm": 4805,
            "presented": audio_frames, "mixed": 4805, "offset": 0, "fade_in": 1.5, "fade_out": 2.5,
            "visual": "immutable", "container_tolerance": .001001}
    result = {"status": "passed", "scope": "three-frame regression of the existing immutable canonical CLI/import path",
              "source": manifest["source"], "mp4_sha256": file_digest(movie), "timing": verify_timing(probe, case),
              "video": verify_pixels(movie, capture, case, None, output),
              "audio": verify_audio(movie, capture, case, load_unmixed(music_wav))}
    write_json(output / "verification.json", result)
    print("immutable-three: finite v2 source, three independently decoded frames, exact video grid and shared audio phase", flush=True)
    return result


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("output", type=Path)
    parser.add_argument("--music-wav", type=Path, required=True)
    parser.add_argument("--native-video", type=Path)
    parser.add_argument("--immutable", action="store_true")
    arguments = parser.parse_args()
    if arguments.immutable:
        verify_immutable(arguments.output.resolve(), arguments.music_wav.resolve())
    else:
        verify(arguments.output.resolve(), arguments.music_wav.resolve(),
               arguments.native_video.resolve() if arguments.native_video else None)
