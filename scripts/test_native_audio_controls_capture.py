#!/usr/bin/env python3
"""Positive and deliberately corrupted PCM controls for the device oracle."""

import array
import json
import math
from pathlib import Path
import shutil
import tempfile
import unittest

from native_audio_controls_capture import qualify

RATE = 48_000
RUNS = [("initial", 0.0, 0.3), ("resumed", 1.4, 2.05), ("seeked", 2.25, 2.75),
        ("restarted", 0.0, 3.69), ("ending", 7.8, 4.20)]
EVENTS = [("volume_quarter", 0.7, 0.4, 0), ("mute", 0.95, 0.65, 0),
          ("unmute", 1.2, 0.9, 0), ("volume_full", 1.45, 1.15, 0),
          ("pause_one", 1.7, 1.4, 0), ("resume", 2.0, 1.4, 0),
          ("rapid_seek", 2.4, 1.75, 1), ("pause_two", 3.1, 2.60, 2),
          ("next_section", 3.4, 4.0, 2), ("previous_section", 3.42, 2.0, 2),
          ("scene_restart", 3.44, 0.0, 2), ("resume_restart", 3.64, 0.0, 2),
          ("seek_end", 4.04, 7.8, 3), ("complete", 4.6, 8.0, 4)]


def carrier(origin):
    result = array.array("f")
    for frame in range(96_000):
        t = origin + frame / RATE
        for frequency, sweep in [(733.0, 71.0), (1319.0, 113.0)]:
            result.append(0.15 * math.sin(math.tau * (frequency * t + sweep * t * t)) if t < 8 else 0)
    return result


def linear(frame, start, frames, before, after):
    if frame <= start:
        return before
    if frame >= start + frames:
        return after
    return before + (after - before) * ((frame - start) / frames)


def generate(directory, *, attack=1920, release=2880, quarter=0.25, phase_error=0.0):
    references = []
    for name, origin, _ in RUNS:
        samples = carrier(origin)
        (directory / f"{name}.f32").write_bytes(samples.tobytes())
        references.append(samples)
    capture = array.array("f", [0.0]) * (5 * RATE * 2)
    release_times = [1.71, 2.41, 3.11, 4.05, None]
    for index, (_, origin, at) in enumerate(RUNS):
        offset = round(at * RATE)
        samples = carrier(origin + phase_error) if index == 1 and phase_error else references[index]
        release_start = round((release_times[index] - at) * RATE) if release_times[index] else None
        count = release_start + release if release_start is not None else round((8 - origin) * RATE)
        for frame in range(count):
            gain = linear(frame, 0, attack, 0, 1)
            if index == 0:
                viewer = 1.0
                for event, before, after in [(0.71, 1, quarter), (0.96, quarter, 0),
                                             (1.21, 0, quarter), (1.46, quarter, 1)]:
                    start = round((event - at) * RATE)
                    if frame >= start:
                        viewer = linear(frame, start, 960, before, after)
                gain *= viewer
            if release_start is not None:
                gain *= linear(frame, release_start, release, 1, 0)
            for channel in (0, 1):
                capture[2 * (offset + frame) + channel] += samples[2 * frame + channel] * gain
    (directory / "capture.f32").write_bytes(capture.tobytes())
    report = {
        "sample_rate": RATE, "channels": 2, "duration": 8.0, "displayed_end_time": 8.0,
        "prepared_silent_seconds": 0.25, "authored_revisions_unchanged": True,
        "rapid_seek_count": 16, "max_live_streams": 2, "releases_retired": True,
        "scene_click_restart": True,
        "runs": [{"name": name, "source_origin": origin, "anchor_seconds": at,
                  "picture_seconds": at - 0.02} for name, origin, at in RUNS],
        "commands": [{"name": name, "wall_seconds": at, "scene_time": time, "run": run}
                     for name, at, time, run in EVENTS],
    }
    (directory / "report.json").write_text(json.dumps(report))


class ControlsCaptureTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.base = tempfile.TemporaryDirectory(prefix="noon-controls-capture-reference-")
        generate(Path(cls.base.name))

    @classmethod
    def tearDownClass(cls):
        cls.base.cleanup()

    def setUp(self):
        self.scratch = tempfile.TemporaryDirectory(prefix="noon-controls-capture-test-")
        self.directory = Path(self.scratch.name)
        shutil.copytree(self.base.name, self.directory, dirs_exist_ok=True)

    def tearDown(self):
        self.scratch.cleanup()

    def corrupt(self, change):
        path = self.directory / "capture.f32"
        samples = array.array("f")
        samples.frombytes(path.read_bytes())
        change(samples)
        path.write_bytes(samples.tobytes())

    def test_known_controls_and_ramps_pass(self):
        report = qualify(self.directory)
        self.assertFalse(report["physical_device_qualified"])
        self.assertEqual(len(report["gain_ramps"]), 13)
        self.assertLess(report["full_reconstruction"][0]["normalized_rmse"], 0.000001)

    def test_clock_observations_between_sparse_alignment_bins_pass(self):
        path = self.directory / "report.json"
        report = json.loads(path.read_text())
        report["runs"][2]["anchor_seconds"] -= 3 / RATE
        report["runs"][4]["anchor_seconds"] += 3 / RATE
        path.write_text(json.dumps(report))
        result = qualify(self.directory)
        self.assertEqual(result["origins_capture_frames"]["seeked"], round(2.75 * RATE))
        self.assertEqual(result["origins_capture_frames"]["ending"], round(4.20 * RATE))

    def test_nearby_replacement_attack_is_not_part_of_old_release(self):
        def earlier_ending(samples):
            first = round(4.20 * RATE) * 2
            length = round(0.20 * RATE) * 2
            target = round(4.17 * RATE) * 2
            ending = samples[first:first + length]
            samples[first:first + length] = array.array("f", [0.0]) * length
            samples[target:target + length] = ending
        self.corrupt(earlier_ending)
        path = self.directory / "report.json"
        report = json.loads(path.read_text())
        report["runs"][4]["anchor_seconds"] -= 0.03
        report["runs"][4]["picture_seconds"] -= 0.03
        path.write_text(json.dumps(report))
        result = qualify(self.directory)
        self.assertAlmostEqual(result["gain_ramps"]["seek_end"]["duration_seconds"], 0.06, places=7)

    def test_swapped_channels_are_rejected(self):
        def swap(samples):
            samples[0::2], samples[1::2] = samples[1::2], samples[0::2]
        self.corrupt(swap)
        with self.assertRaises(ValueError):
            qualify(self.directory)

    def test_resumed_wrong_source_phase_is_rejected(self):
        generate(self.directory, phase_error=0.02)
        with self.assertRaises(ValueError):
            qualify(self.directory)

    def test_half_length_attack_is_rejected(self):
        generate(self.directory, attack=960)
        with self.assertRaisesRegex(ValueError, "attack"):
            qualify(self.directory)

    def test_extended_release_is_rejected(self):
        generate(self.directory, release=5760)
        with self.assertRaisesRegex(ValueError, "ramp"):
            qualify(self.directory)

    def test_abrupt_release_is_rejected(self):
        generate(self.directory, release=1)
        with self.assertRaisesRegex(ValueError, "intermediate"):
            qualify(self.directory)

    def test_wrong_viewer_gain_is_rejected(self):
        generate(self.directory, quarter=0.5)
        with self.assertRaises(ValueError):
            qualify(self.directory)

    def test_preparation_sound_is_rejected(self):
        def leak(samples):
            for index in range(4800, 9600):
                samples[index] = 0.02
        self.corrupt(leak)
        with self.assertRaises(ValueError):
            qualify(self.directory)

    def test_obsolete_seek_voice_is_rejected(self):
        def leak(samples):
            for index in range(round(2.55 * RATE) * 2, round(2.65 * RATE) * 2):
                samples[index] = 0.02
        self.corrupt(leak)
        with self.assertRaises(ValueError):
            qualify(self.directory)

    def test_mute_leak_is_rejected(self):
        def leak(samples):
            for index in range(round(1.02 * RATE) * 2, round(1.12 * RATE) * 2):
                samples[index] = 0.01
        self.corrupt(leak)
        with self.assertRaises(ValueError):
            qualify(self.directory)


if __name__ == "__main__":
    unittest.main()
