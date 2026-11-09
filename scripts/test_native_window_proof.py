#!/usr/bin/env python3
"""Pixel/lifecycle negative controls; never connect to or create a display."""
import copy
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest
from unittest import mock


SPEC = importlib.util.spec_from_file_location("native_window_proof", Path(__file__).with_name("native_window_proof.py"))
PROOF = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(PROOF)


def lesson_pixels(square=141.5, circle=177.5, *, title=True, equation=True, caption=False, y_center=98.5):
    """Independent small raster scene with separate shape and text regions."""
    pixels = bytearray([18, 18, 20] * (320 * 180))

    def paint(x, y, color):
        offset = (y * 320 + x) * 3
        pixels[offset:offset + 3] = bytes(color)

    for enabled, top, left, right in ((title, 25, 90, 230), (equation, 52, 115, 205),
                                      (caption, 132, 100, 220)):
        if enabled:
            for y in range(top, top + 3):
                for x in range(left, right):
                    if x % 7 < 4:
                        paint(x, y, (245, 245, 245))
    for y in range(180):
        for x in range(320):
            if abs(x - square) <= 8.5 and abs(y - y_center) <= 8.5:
                paint(x, y, (70, 170, 235))
            if (x - circle) ** 2 + (y - y_center) ** 2 <= 9 ** 2:
                paint(x, y, (240, 140, 40))
    return bytes(pixels)


def frame(seconds, square=141.5, circle=177.5, **options):
    return PROOF.inspect_lesson_frame(lesson_pixels(square, circle, **options), seconds)


def healthy_observations():
    # Deliberately sparse and irregular: qualify observable behavior, not FPS.
    return [frame(0.2), frame(7.5, 158.5, 194.5),
            frame(9.5, 175.25, 211.25, caption=True),
            frame(10.75, 175.25, 211.25, caption=True)]


class NativePixels(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.healthy = healthy_observations()

    def setUp(self):
        self.observations = copy.deepcopy(self.healthy)

    def reject(self, message):
        with self.assertRaisesRegex(PROOF.NativeWindowProofError, message):
            PROOF.validate_observations(self.observations)

    def test_one_intermediate_and_irregular_sampling_are_sufficient(self):
        result = PROOF.validate_observations(self.observations)
        self.assertTrue(result["complete"])
        self.assertEqual(result["final_alive_seconds"], 1.25)
        self.assertEqual(result["intermediate_seconds"], 7.5)

    def test_repeated_presented_frames_are_allowed(self):
        duplicate = copy.deepcopy(self.observations[1])
        duplicate["seconds"] = 8.0
        self.observations.insert(2, duplicate)
        self.assertTrue(PROOF.validate_observations(self.observations)["complete"])

    def test_blank_window_fails(self):
        self.observations[1] = PROOF.inspect_lesson_frame(bytes(320 * 180 * 3), 7.5)
        self.reject("title is missing")

    def test_static_shapes_cannot_satisfy_motion(self):
        self.observations = [frame(0.2), frame(7.5), frame(9.5), frame(10.75)]
        self.reject("intermediate native motion")

    def test_title_does_not_substitute_for_equation(self):
        self.observations[1] = frame(7.5, 158.5, 194.5, equation=False)
        self.assertGreater(self.observations[1]["title_white_pixels"], 20)
        self.reject("equation is missing")

    def test_missing_title_fails(self):
        self.observations[1] = frame(7.5, 158.5, 194.5, title=False)
        self.reject("title is missing")

    def test_missing_late_caption_fails(self):
        self.observations[2:] = [frame(9.5, 175.25, 211.25), frame(10.75, 175.25, 211.25)]
        self.reject("late caption were not observed")

    def test_caption_before_final_positions_fails(self):
        self.observations[1] = frame(7.5, 158.5, 194.5, caption=True)
        self.reject("caption appeared before")

    def test_only_one_shape_moving_fails(self):
        self.observations[1] = frame(7.5, 158.5, 177.5)
        self.reject("did not move together")

    def test_missing_shape_fails(self):
        # Draw the circle outside the canvas, retaining the square and text.
        self.observations[1] = frame(7.5, 158.5, 500.0)
        self.reject("circle is missing")

    def test_parallel_but_backwards_motion_fails(self):
        self.observations[1:2] = [frame(4.0, 165.5, 201.5), frame(7.5, 149.5, 185.5)]
        self.reject("motion is out of order")

    def test_jump_from_initial_to_final_fails(self):
        del self.observations[1]
        self.reject("intermediate native motion")

    def test_missing_initial_state_fails(self):
        del self.observations[0]
        self.reject("initial README shape positions")

    def test_wrong_vertical_position_fails(self):
        self.observations[1] = frame(7.5, 158.5, 194.5, y_center=88.5)
        self.reject("vertical position differs")

    def test_caption_must_remain_during_final_hold(self):
        self.observations[-1] = frame(10.75, 175.25, 211.25)
        self.reject("final state did not remain visible")

    def test_final_hold_shorter_than_one_second_fails(self):
        self.observations[-1]["seconds"] = 10.4
        self.reject("at least one second")

    def test_nonfinite_or_unordered_observation_times_fail(self):
        for stamp in (float("nan"), float("inf"), -1, 0.2):
            with self.subTest(stamp=stamp):
                self.observations[1]["seconds"] = stamp
                self.reject("strictly ordered")


class NativeLifecycle(unittest.TestCase):
    def setUp(self):
        self.lifecycle = {"close_protocol": "WM_DELETE_WINDOW", "close_sent": True,
                          "alive_before_close": True, "forced_teardown": False,
                          "returncode": 0, "final_seconds": 2.0,
                          "last_observed_seconds": 3.1, "close_seconds": 3.2, "exit_seconds": 3.3}

    def test_protocol_close_and_zero_exit_pass(self):
        PROOF.validate_lifecycle(self.lifecycle)

    def test_nonzero_and_signal_exit_fail(self):
        for code in (1, -15, -9, None, False):
            with self.subTest(code=code):
                self.lifecycle["returncode"] = code
                with self.assertRaisesRegex(PROOF.NativeWindowProofError, "status 0"):
                    PROOF.validate_lifecycle(self.lifecycle)

    def test_forced_teardown_cannot_pass_even_with_zero_exit(self):
        self.lifecycle["forced_teardown"] = True
        with self.assertRaisesRegex(PROOF.NativeWindowProofError, "forced native process teardown"):
            PROOF.validate_lifecycle(self.lifecycle)

    def test_missing_protocol_or_early_natural_exit_fails(self):
        for key, value in (("close_protocol", "XDestroyWindow"), ("close_sent", False),
                           ("alive_before_close", False)):
            with self.subTest(key=key):
                broken = dict(self.lifecycle, **{key: value})
                with self.assertRaisesRegex(PROOF.NativeWindowProofError, "protocol close"):
                    PROOF.validate_lifecycle(broken)

    def test_close_before_hold_or_exit_before_close_fails(self):
        for key, value in (("last_observed_seconds", 2.9), ("close_seconds", 3.0),
                           ("exit_seconds", 3.0)):
            with self.subTest(key=key):
                broken = dict(self.lifecycle, **{key: value})
                with self.assertRaisesRegex(PROOF.NativeWindowProofError, "retained final state"):
                    PROOF.validate_lifecycle(broken)


class X11ImageFormat(unittest.TestCase):
    def setUp(self):
        self.layout = {"depth": 24, "bits_per_pixel": 32, "scanline_pad": 32,
                       "image_byte_order": 0, "visual_class": 4,
                       "red_mask": 0xFF0000, "green_mask": 0xFF00, "blue_mask": 0xFF}

    def test_bgrx_pixels_decode_with_alpha_ignored(self):
        native = bytes([30, 20, 10, 0, 90, 80, 70, 255])
        expected = bytes([10, 20, 30, 70, 80, 90])
        self.assertEqual(PROOF.decode_x11_rgb(native, 2, 1, self.layout), expected)
        self.layout["depth"] = 32
        self.assertEqual(PROOF.decode_x11_rgb(native, 2, 1, self.layout), expected)

    def test_unverified_layouts_fail(self):
        for key, value in (("depth", 16), ("bits_per_pixel", 24), ("scanline_pad", 64),
                           ("image_byte_order", 1), ("visual_class", 3), ("red_mask", 0xFF)):
            with self.subTest(key=key):
                broken = dict(self.layout, **{key: value})
                with self.assertRaisesRegex(PROOF.NativeWindowProofError, "unsupported X11"):
                    PROOF.decode_x11_rgb(bytes(4), 1, 1, broken)

    def test_truncated_image_is_rejected(self):
        with self.assertRaisesRegex(PROOF.NativeWindowProofError, "byte count"):
            PROOF.decode_x11_rgb(bytes(7), 2, 1, self.layout)

    def test_normalization_uses_native_pixel_centers(self):
        native = bytearray(960 * 540 * 3)
        native[(1 * 960 + 1) * 3:(1 * 960 + 1) * 3 + 3] = b"\x11\x22\x33"
        native[(538 * 960 + 958) * 3:(538 * 960 + 958) * 3 + 3] = b"\x44\x55\x66"
        normalized = PROOF.normalize_rgb(native, 960, 540)
        self.assertEqual(normalized[:3], b"\x11\x22\x33")
        self.assertEqual(normalized[-3:], b"\x44\x55\x66")
        with self.assertRaisesRegex(PROOF.NativeWindowProofError, "default 960x540"):
            PROOF.normalize_rgb(bytes(640 * 360 * 3), 640, 360)


class FailureEvidence(unittest.TestCase):
    def test_missing_display_writes_failure_report_without_launch_or_xlib(self):
        with tempfile.TemporaryDirectory() as directory:
            evidence = Path(directory)
            (evidence / "README.md").write_text("retained upstream README", encoding="utf8")
            with mock.patch.object(PROOF.subprocess, "Popen") as launch, \
                    mock.patch.object(PROOF.importlib.metadata, "version") as version:
                with self.assertRaisesRegex(PROOF.NativeWindowProofError, "existing local DISPLAY"):
                    PROOF.qualify_window("/not-launched", evidence, {}, cwd=directory)
                launch.assert_not_called()
                version.assert_not_called()
            report = json.loads((evidence / "report.json").read_text(encoding="utf8"))
            self.assertEqual(report["status"], "failed")
            self.assertFalse(report["lifecycle"]["close_sent"])
            self.assertEqual((evidence / "README.md").read_text(encoding="utf8"), "retained upstream README")


if __name__ == "__main__":
    unittest.main()
