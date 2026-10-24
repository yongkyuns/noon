#!/usr/bin/env python3
"""Qualify native viewer controls from recorded PCM and shared-mixer references."""

import argparse
import array
import json
import math
from pathlib import Path
import sys

from native_audio_capture import compare, first_sound, pcm, quiet, require

RATE = 48_000
ATTACK = 0.040
RELEASE = 0.060
VIEWER_RAMP = 0.020
MAX_CLOCK_ERROR = 0.050


def align(reference, captured, rough):
    """Recover sample alignment using both distinct carriers after the attack."""
    indices = [2 * frame + channel for frame in range(3840, 7680, 12) for channel in (0, 1)]
    reference_power = sum(reference[index] ** 2 for index in indices)

    def score(shift):
        if 2 * shift + indices[0] < 0 or 2 * shift + indices[-1] >= len(captured):
            return -2.0
        values = [captured[2 * shift + index] for index in indices]
        power = sum(value * value for value in values)
        if power == 0:
            return -2.0
        return sum(reference[index] * value for index, value in zip(indices, values)) / math.sqrt(
            reference_power * power
        )

    bound = round(MAX_CLOCK_ERROR * RATE)
    # Sweep every frame inside the unchanged clock-error bound. Sparse guesses
    # can prefer a carrier-phase alias when the device observation lies between
    # their bins, especially at the higher-frequency seek target.
    return max(range(rough - bound, rough + bound + 1), key=score)


def verify_pcm(reference, actual, name):
    result = compare(reference, actual)
    require(result["correlation"] >= 0.995, f"{name}: waveform/source phase mismatch: {result}")
    require(0.95 <= result["gain_ratio"] <= 1.05, f"{name}: unexpected output gain: {result}")
    require(result["normalized_rmse"] <= 0.1, f"{name}: PCM error exceeds tolerance: {result}")
    return result


def observe_ramp(reference, captured, origin, begin, end, before, after, duration, name):
    """Measure a linear ramp's slope; no callback state or ramp code is reused."""
    observations = []
    for frame in range(max(0, begin), min(end, len(reference) // 2)):
        index = 2 * frame
        actual_index = 2 * (origin + frame)
        if actual_index < 0 or actual_index + 1 >= len(captured):
            continue
        power = reference[index] ** 2 + reference[index + 1] ** 2
        if power < 0.00001:
            continue
        gain = (reference[index] * captured[actual_index]
                + reference[index + 1] * captured[actual_index + 1]) / power
        progress = (gain - before) / (after - before)
        if 0.1 < progress < 0.9:
            observations.append((frame, progress))
    expected_frames = round(duration * RATE)
    require(len(observations) >= 0.60 * expected_frames,
            f"{name}: missing intermediate gain samples; transition may be abrupt")
    xmean = sum(item[0] for item in observations) / len(observations)
    ymean = sum(item[1] for item in observations) / len(observations)
    variance = sum((x - xmean) ** 2 for x, _ in observations)
    require(variance > 0, f"{name}: no measurable ramp interval")
    slope = sum((x - xmean) * (y - ymean) for x, y in observations) / variance
    require(slope > 0, f"{name}: gain moved in the wrong direction")
    measured_frames = 1 / slope
    intercept = ymean - slope * xmean
    start = -intercept / slope
    residual = math.sqrt(sum((y - (slope * x + intercept)) ** 2
                             for x, y in observations) / len(observations))
    require(abs(measured_frames - expected_frames) <= 2,
            f"{name}: measured ramp {measured_frames / RATE:.9f}s, expected {duration:.3f}s")
    require(residual < 0.002, f"{name}: non-linear/discontinuous gain envelope: {residual}")
    require(abs(start - round(start)) < 0.1, f"{name}: ramp is not on one sample grid")
    return {
        "start_frame": round(start), "duration_seconds": measured_frames / RATE,
        "linear_fit_rmse": residual, "intermediate_samples": len(observations),
        "before": before, "after": after,
    }


def ramp_value(frame, start, duration, before, after):
    progress = min(1.0, max(0.0, (frame - start) / duration))
    return before + (after - before) * progress


def qualify(directory):
    directory = Path(directory)
    report = json.loads((directory / "report.json").read_text())
    require(report["sample_rate"] == RATE and report["channels"] == 2, "Unexpected proof format")
    require(report["duration"] == 8.0 and report["displayed_end_time"] == 8.0,
            "Controls did not reach the exact authored endpoint")
    require(report["prepared_silent_seconds"] >= 0.15, "Missing silent preparation")
    require(report["authored_revisions_unchanged"] is True, "Viewer controls changed authored state")
    require(report["rapid_seek_count"] == 16 and report["max_live_streams"] == 2,
            "Rapid seeks were not bounded to current plus releasing output")
    require(report["releases_retired"] is True and report["scene_click_restart"] is True,
            "Missing release retirement or shared scene-click dispatch")
    runs = report["runs"]
    require([run["name"] for run in runs] == ["initial", "resumed", "seeked", "restarted", "ending"],
            "Unexpected stream sequence")
    commands = {command["name"]: command for command in report["commands"]}
    require(runs[0]["source_origin"] == 0 and runs[2]["source_origin"] == 2.25
            and runs[3]["source_origin"] == 0 and runs[4]["source_origin"] == 7.8,
            "Seek/restart source origins differ from the commanded targets")
    require(abs(runs[1]["source_origin"] - commands["pause_one"]["scene_time"]) < 1e-9,
            "Resume lost the exact paused source offset")
    require([commands[name]["scene_time"] for name in
             ("next_section", "previous_section", "scene_restart")] == [4.0, 2.0, 0.0],
            "Section navigation or the actual Restart click selected the wrong target")
    captured, capture_hash = pcm(directory / "capture.f32")
    require(len(captured) % 2 == 0, "Incomplete captured stereo frame")
    references = []
    hashes = {}
    origins = []
    for run in runs:
        reference, digest = pcm(directory / f'{run["name"]}.f32')
        require(len(reference) == 2 * 96_000, "Reference must contain two exact authored seconds")
        references.append(reference)
        hashes[run["name"]] = digest
        rough = (first_sound(captured) - first_sound(reference) if not origins else
                 origins[0] + round((run["anchor_seconds"] - runs[0]["anchor_seconds"]) * RATE))
        origin = align(reference, captured, rough)
        require(abs(origin - rough) <= round(MAX_CLOCK_ERROR * RATE),
                f'{run["name"]}: sample anchor disagrees with the native clock by more than 50ms')
        require(run["anchor_seconds"] + 0.001 >= run["picture_seconds"],
                f'{run["name"]}: output preceded the completed picture')
        origins.append(origin)
    require(origins[0] >= 0.15 * RATE, "Capture lacks preparation silence")
    require(origins[-1] + round(0.35 * RATE) <= len(captured) // 2, "Capture lacks completion silence")

    models = []
    measured = {}
    steady = {}
    release_commands = ["pause_one", "rapid_seek", "pause_two", "seek_end", None]
    viewer_commands = [("volume_quarter", 1.0, 0.25), ("mute", 0.25, 0.0),
                       ("unmute", 0.0, 0.25), ("volume_full", 0.25, 1.0)]
    for index, (run, reference, origin) in enumerate(zip(runs, references, origins)):
        attack_name = f'{run["name"]}_attack'
        attack = observe_ramp(reference, captured, origin, 0, round(0.075 * RATE),
                              0, 1, ATTACK, attack_name)
        require(abs(attack["start_frame"]) <= 2, f"{attack_name}: waveform offset disagrees with attack")
        measured[attack_name] = attack
        model = {"attack": attack, "viewer": [], "release": None,
                 "end": min(96_000, round((8.0 - run["source_origin"]) * RATE))}
        for channel in (0, 1):
            begin, end = round(0.08 * RATE), round(0.16 * RATE)
            steady[f'{run["name"]}_channel_{channel}'] = verify_pcm(
                reference[2 * begin + channel:2 * end:2],
                captured[2 * (origin + begin) + channel:2 * (origin + end):2],
                f'{run["name"]} channel {channel}',
            )
        if index == 0:
            for name, before, after in viewer_commands:
                at = round((commands[name]["wall_seconds"] - run["anchor_seconds"]) * RATE)
                transition = observe_ramp(reference, captured, origin,
                                          at - round(0.04 * RATE), at + round(0.15 * RATE),
                                          before, after, VIEWER_RAMP, name)
                measured[name] = transition
                model["viewer"].append(transition)
        release_name = release_commands[index]
        if release_name:
            at = round((commands[release_name]["wall_seconds"] - run["anchor_seconds"]) * RATE)
            # A later replacement has its own source phase and attack. Do not
            # include those samples in the retiring voice's slope observation.
            end = min(at + round(0.15 * RATE), origins[index + 1] - origin)
            release = observe_ramp(reference, captured, origin,
                                  at - round(0.04 * RATE), end,
                                  1, 0, RELEASE, release_name)
            measured[release_name] = release
            model["release"] = release
            model["end"] = release["start_frame"] + round(RELEASE * RATE)
        models.append(model)

    # Reconstruct all recorded output, including ramps and every quiet interval.
    # The only inferred values are device/callback arrival frames. Durations,
    # target gains, source positions and PCM remain independent expectations.
    expected = array.array("f", [0.0]) * len(captured)
    for reference, origin, model in zip(references, origins, models):
        for frame in range(model["end"]):
            gain = ramp_value(frame, 0, ATTACK * RATE, 0, 1)
            viewer = 1.0
            for transition in model["viewer"]:
                if frame >= transition["start_frame"]:
                    viewer = ramp_value(frame, transition["start_frame"], VIEWER_RAMP * RATE,
                                        transition["before"], transition["after"])
            gain *= viewer
            if model["release"]:
                gain *= ramp_value(frame, model["release"]["start_frame"], RELEASE * RATE, 1, 0)
            for channel in (0, 1):
                expected[2 * (origin + frame) + channel] += reference[2 * frame + channel] * gain
    reconstructed = [verify_pcm(expected[channel::2], captured[channel::2], f"Full capture channel {channel}")
                     for channel in (0, 1)]
    silence = {"preparation": quiet(captured, 0, origins[0], "Preparation")}
    for index, model in enumerate(models):
        begin = origins[index] + model["end"]
        end = origins[index + 1] if index + 1 < len(origins) else len(captured) // 2
        silence[f'after_{runs[index]["name"]}'] = quiet(captured, begin, end, f"Retirement gap {index}")
    silence["viewer_mute"] = quiet(
        captured, origins[0] + measured["mute"]["start_frame"] + round(VIEWER_RAMP * RATE),
        origins[0] + measured["unmute"]["start_frame"], "Viewer mute",
    )
    result = {
        "sample_rate": RATE, "captured_frames": len(captured) // 2,
        "origins_capture_frames": dict(zip([run["name"] for run in runs], origins)),
        "steady_pcm": steady, "full_reconstruction": reconstructed,
        "gain_ramps": measured, "silence_rms": silence,
        "reference_sha256": hashes, "capture_sha256": capture_hash,
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
    except (OSError, ValueError, KeyError, IndexError) as error:
        print(f"Native audio controls capture failed: {error}", file=sys.stderr)
        return 1
    print(json.dumps(result, indent=2))
    return 0


if __name__ == "__main__":
    sys.exit(main())
