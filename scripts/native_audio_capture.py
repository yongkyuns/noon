#!/usr/bin/env python3
"""Compare paced native device capture against the shared mixer's exact PCM."""

import argparse
from array import array
import hashlib
import json
import math
from pathlib import Path
import sys


def require(condition, message):
    if not condition:
        raise ValueError(message)


def pcm(path):
    payload = path.read_bytes()
    require(len(payload) % 8 == 0, f"{path.name}: incomplete stereo PCM frame")
    samples = array("f")
    samples.frombytes(payload)
    if sys.byteorder != "little":
        samples.byteswap()
    require(all(math.isfinite(value) for value in samples), f"{path.name}: nonfinite PCM")
    return samples, hashlib.sha256(payload).hexdigest()


def first_sound(samples, threshold=0.0001):
    for frame in range(len(samples) // 2):
        if max(abs(samples[2 * frame]), abs(samples[2 * frame + 1])) > threshold:
            return frame
    raise ValueError("PCM contains no audible marker")


def compare(reference, actual):
    require(len(reference) == len(actual) and reference, "PCM comparison has no complete interval")
    reference_power = sum(value * value for value in reference)
    actual_power = sum(value * value for value in actual)
    require(reference_power > 0 and actual_power > 0, "PCM comparison contains no signal")
    dot = sum(a * b for a, b in zip(reference, actual))
    error_power = sum((a - b) ** 2 for a, b in zip(reference, actual))
    return {
        "correlation": dot / math.sqrt(reference_power * actual_power),
        "gain_ratio": math.sqrt(actual_power / reference_power),
        "normalized_rmse": math.sqrt(error_power / reference_power),
    }


def align(expected, captured, rate):
    # Locate the first known marker, then refine with both distinct channel tones.
    # Compilation/startup silence is not assumed to have a fixed duration.
    rough = first_sound(captured) - first_sound(expected)
    begin, end = int(0.095 * rate), int(0.305 * rate)
    indices = [2 * frame + channel for frame in range(begin, end, 8) for channel in (0, 1)]
    expected_power = sum(expected[index] ** 2 for index in indices)

    def score(shift):
        # A negative origin diagnoses a missing leading prefix. Qualification
        # still rejects it below; do not mistake a later carrier-phase match for
        # a valid origin when the monitor started after authored time zero.
        if 2 * shift + indices[0] < 0 or 2 * shift + indices[-1] >= len(captured):
            return -2.0
        values = [captured[2 * shift + index] for index in indices]
        power = sum(value * value for value in values)
        if power == 0:
            return -2.0
        return sum(expected[index] * value for index, value in zip(indices, values)) / math.sqrt(
            expected_power * power
        )

    coarse = max(range(rough - 1024, rough + 1025, 8), key=score)
    return max(range(coarse - 8, coarse + 9), key=score)


def quiet(captured, begin, end, name):
    require(0 <= begin < end <= len(captured) // 2, f"{name}: captured interval is incomplete")
    samples = captured[2 * begin:2 * end]
    rms = math.sqrt(sum(sample * sample for sample in samples) / len(samples))
    require(rms < 0.00001, f"{name}: expected silence, measured RMS {rms:.6f}")
    return rms


def qualify(directory):
    directory = Path(directory)
    report = json.loads((directory / "report.json").read_text())
    require(report["sample_rate"] == 48000 and report["channels"] == 2, "Unexpected device proof format")
    require(report["duration"] == 1.0, "Unexpected device proof duration")
    require(report["prepared_silent_seconds"] >= 0.15, "Fixture did not exercise silent preparation")
    require(abs(report["displayed_end_time"] - 1.0) < 1.0e-9, "Picture did not reach its exact endpoint")
    rate = report["sample_rate"]
    expected, expected_hash = pcm(directory / "expected.f32")
    captured, captured_hash = pcm(directory / "capture.f32")
    require(len(expected) == 2 * rate, "Expected PCM does not cover the exact authored second")
    origin = align(expected, captured, rate)
    require(origin >= int(0.15 * rate),
            f"Capture is missing silent preparation before playback (authored origin {origin} frames)")
    require(origin + int(1.15 * rate) <= len(captured) // 2, "Capture is missing post-retirement silence")
    actual = captured[2 * origin:2 * (origin + rate)]
    channels = []
    for channel in (0, 1):
        result = compare(expected[channel::2], actual[channel::2])
        require(result["correlation"] >= 0.995, f"Channel {channel}: PCM timing or identity mismatch: {result}")
        require(0.95 <= result["gain_ratio"] <= 1.05, f"Channel {channel}: unexpected output gain: {result}")
        require(result["normalized_rmse"] <= 0.1, f"Channel {channel}: PCM error exceeds tolerance: {result}")
        channels.append(result)
    silence = {
        "preparation": quiet(captured, 0, origin, "Preparation"),
        "gap": quiet(captured, origin + int(0.33 * rate), origin + int(0.52 * rate), "Authored gap"),
        "end": quiet(captured, origin + int(0.85 * rate), origin + int(1.15 * rate), "End and retirement"),
    }
    result = {
        "sample_rate": rate,
        "channels": channels,
        "captured_frames": len(captured) // 2,
        "authored_origin_capture_frame": origin,
        "silence_rms": silence,
        "expected_sha256": expected_hash,
        "capture_sha256": captured_hash,
        "device": "private PulseAudio null sink through ALSA",
        "physical_device_qualified": False,
    }
    (directory / "analysis.json").write_text(json.dumps(result, indent=2) + "\n")
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("proof_directory")
    args = parser.parse_args()
    try:
        result = qualify(args.proof_directory)
    except (OSError, ValueError, KeyError) as error:
        print(f"Native audio capture failed: {error}", file=sys.stderr)
        return 1
    print(json.dumps(result, indent=2))
    return 0


if __name__ == "__main__":
    sys.exit(main())
