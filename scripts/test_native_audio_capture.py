#!/usr/bin/env python3
"""Independent negative controls for the native device capture qualification."""

from array import array
import json
import math
from pathlib import Path
import sys
import tempfile
import unittest

from native_audio_capture import qualify


RATE = 48_000


def fixture():
    expected = array("f")
    for frame in range(RATE):
        time = frame / RATE
        if 0.1 <= time < 0.3:
            start, end, frequencies = 0.1, 0.3, (997, 1499)
        elif 0.55 <= time < 0.8:
            start, end, frequencies = 0.55, 0.8, (1999, 701)
        else:
            expected.extend((0.0, 0.0))
            continue
        gain = 0.15 * min(1.0, (time - start) / 0.005, (end - time) / 0.005)
        expected.extend(gain * math.sin(math.tau * frequency * (time - start)) for frequency in frequencies)
    return expected


def write_pcm(path, samples):
    encoded = array("f", samples)
    if sys.byteorder != "little":
        encoded.byteswap()
    path.write_bytes(encoded.tobytes())


class NativeAudioCaptureTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="noon-audio-capture-")
        self.addCleanup(self.temporary.cleanup)
        self.directory = Path(self.temporary.name)
        self.expected = fixture()
        self.origin = int(0.2 * RATE)
        self.capture = array("f", [0.0]) * (2 * self.origin)
        self.capture.extend(self.expected)
        self.capture.extend([0.0] * (2 * int(0.2 * RATE)))
        write_pcm(self.directory / "expected.f32", self.expected)
        (self.directory / "report.json").write_text(json.dumps({
            "sample_rate": RATE, "channels": 2, "duration": 1.0,
            "prepared_silent_seconds": 0.15, "displayed_end_time": 1.0,
        }))

    def check_capture(self):
        write_pcm(self.directory / "capture.f32", self.capture)
        return qualify(self.directory)

    def test_exact_mix_is_located_after_variable_startup_silence(self):
        result = self.check_capture()
        self.assertEqual(result["authored_origin_capture_frame"], self.origin)
        self.assertEqual(result["silence_rms"], {"preparation": 0.0, "gap": 0.0, "end": 0.0})
        self.assertFalse(result["physical_device_qualified"])
        self.assertTrue((self.directory / "analysis.json").is_file())

    def test_swapped_channels_are_rejected(self):
        for frame in range(len(self.capture) // 2):
            self.capture[2 * frame], self.capture[2 * frame + 1] = self.capture[2 * frame + 1], self.capture[2 * frame]
        with self.assertRaisesRegex(ValueError, "identity mismatch"):
            self.check_capture()

    def test_clock_drift_between_markers_is_rejected(self):
        begin = 2 * (self.origin + int(0.5 * RATE))
        self.capture[begin:begin] = array("f", [0.0]) * (2 * 120)
        with self.assertRaisesRegex(ValueError, "identity mismatch"):
            self.check_capture()

    def test_low_level_audio_in_authored_gap_is_rejected(self):
        for frame in range(self.origin + int(0.34 * RATE), self.origin + int(0.5 * RATE)):
            self.capture[2 * frame] = 0.0001
        with self.assertRaisesRegex(ValueError, "Authored gap"):
            self.check_capture()

    def test_audio_after_retirement_is_rejected(self):
        for frame in range(self.origin + RATE, self.origin + int(1.15 * RATE)):
            self.capture[2 * frame] = 0.01
        with self.assertRaisesRegex(ValueError, "End and retirement"):
            self.check_capture()

    def test_empty_device_output_is_rejected(self):
        self.capture = array("f", [0.0]) * len(self.capture)
        with self.assertRaisesRegex(ValueError, "no audible marker"):
            self.check_capture()

    def test_truncated_end_observation_is_rejected(self):
        del self.capture[2 * (self.origin + RATE):]
        with self.assertRaisesRegex(ValueError, "post-retirement silence"):
            self.check_capture()

    def test_missing_preparation_and_authored_silence_reports_the_actual_negative_origin(self):
        del self.capture[:2 * (self.origin + 337)]
        with self.assertRaisesRegex(ValueError, "authored origin -337 frames"):
            self.check_capture()


if __name__ == "__main__":
    unittest.main()
