"""Real native Python MP4 qualification; requires built extension, GPU and FFmpeg.

Run with PYTHONPATH=build/python python3 scripts/test_native_python_export.py.
The output is inspected with ffprobe and decoded pixels, not just file presence.
"""
import asyncio
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

import noon_native
from noon_native import ExportCancellation, prepare_view, run_source


WIDTH, HEIGHT = 160, 90
RATE_NUMERATOR, RATE_DENOMINATOR = 30_000, 1_001
FRAME_BYTES = WIDTH * HEIGHT * 4
SOURCE = '''from noon import *
context["module"] += 1
class ExportLesson(Scene):
    def construct(self):
        context["construct"] += 1
        marker = Square(0.8, fill=WHITE, fill_opacity=1, stroke=None).shift(2 * LEFT)
        self.add(marker)
        self.play(marker.animate.shift(4 * RIGHT), run_time=0.6, rate_func=linear)
        context["completed"] += 1
        # Use saturated red for the strict decoded-pixel oracle.
        marker.set_fill(Color(1, 0, 0))
        self.wait(0.13)
'''


def prepare(source=SOURCE):
    context = {"module": 0, "construct": 0, "completed": 0}
    scene = asyncio.run(run_source(source, context, retain_history=True))
    return prepare_view(scene), context


def export(view, path, **options):
    return view.export_mp4(path, fps=RATE_NUMERATOR, fps_denominator=RATE_DENOMINATOR,
                           width=WIDTH, height=HEIGHT, **options)


def probe(path):
    process = subprocess.run([
        "ffprobe", "-v", "error", "-show_streams", "-show_packets", "-of", "json", str(path),
    ], check=True, capture_output=True)
    return json.loads(process.stdout)


def decode(path):
    return subprocess.run([
        "ffmpeg", "-v", "error", "-i", str(path), "-map", "0:v:0", "-fps_mode", "passthrough",
        "-f", "rawvideo", "-pix_fmt", "rgba", "pipe:1",
    ], check=True, capture_output=True).stdout


class NativePythonExportTests(unittest.TestCase):
    def assert_movie(self, path, count, *, start_frame=0, duration=None):
        metadata = probe(path)
        self.assertEqual(len(metadata["streams"]), 1, "native Python must not silently add an audio stream")
        stream = metadata["streams"][0]
        self.assertEqual(stream["codec_name"], "h264")
        self.assertEqual((stream["width"], stream["height"]), (WIDTH, HEIGHT))
        self.assertEqual(stream["time_base"], "1/30000")
        self.assertEqual(int(stream["nb_frames"]), count)
        self.assertEqual(float(stream["start_time"]), 0)
        duration_ticks = round(duration * RATE_NUMERATOR) if duration is not None else count * RATE_DENOMINATOR
        self.assertEqual(stream["duration_ts"], duration_ticks)
        packets = metadata["packets"]
        self.assertEqual(len(packets), count)
        self.assertEqual([packet["pts"] for packet in packets],
                         [index * RATE_DENOMINATOR for index in range(count)])
        self.assertEqual([packet["duration"] for packet in packets[:-1]], [RATE_DENOMINATOR] * (count - 1))
        self.assertEqual(packets[-1]["duration"], duration_ticks - (count - 1) * RATE_DENOMINATOR)

        pixels = decode(path)
        self.assertEqual(len(pixels), count * FRAME_BYTES)
        for index in range(count):
            frame = pixels[index * FRAME_BYTES:(index + 1) * FRAME_BYTES]
            time = (start_frame + index) * RATE_DENOMINATOR / RATE_NUMERATOR
            if time < 0.6:
                xs = [offset // 4 % WIDTH for offset in range(0, FRAME_BYTES, 4)
                      if min(frame[offset:offset + 3]) > 160]
            else:
                xs = [offset // 4 % WIDTH for offset in range(0, FRAME_BYTES, 4)
                      if frame[offset] > 160 and frame[offset + 1] < 80 and frame[offset + 2] < 80]
            self.assertGreater(len(xs), 30, f"missing marker/color at output frame {index}")
            expected_x = 79.5 + 11.25 * (-2 + 4 * min(time / 0.6, 1))
            self.assertAlmostEqual(sum(xs) / len(xs), expected_x, delta=0.85,
                                   msg=f"wrong source sample/order at output frame {index}")

    def test_source_runs_once_and_off_grid_endpoint_preserves_actual_mp4_timing(self):
        view, context = prepare()
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "lesson.mp4"
            # Exercise exporting a view that previously inspected a later time.
            view.capture(time=0.65, width=WIDTH, height=HEIGHT)
            summary = export(view, path)
            self.assertEqual((summary.frames, summary.fps, summary.fps_denominator), (22, 30_000, 1_001))
            self.assertEqual(summary.reason, "source_end")
            self.assertAlmostEqual(summary.source_end, 0.73)
            self.assertEqual(context, {"module": 1, "construct": 1, "completed": 1})
            self.assert_movie(path, 22, duration=0.73)
            self.assertEqual([entry.name for entry in Path(directory).iterdir()], ["lesson.mp4"])
            with self.assertRaisesRegex(RuntimeError, "closed"):
                view.capture(time=0, width=WIDTH, height=HEIGHT)
        view.close()

    def test_crops_end_bounds_and_explicit_holds_share_the_native_frame_policy(self):
        cases = [
            ({"start_frame": 3, "frames": 5}, 5, 3, None),
            ({"end_time": 0.21875}, 7, 0, None),
            ({"start_frame": 7}, 15, 7, 0.73 - 7 * RATE_DENOMINATOR / RATE_NUMERATOR),
            ({"hold": 0.1}, 25, 0, None),
        ]
        with tempfile.TemporaryDirectory() as directory:
            for index, (options, count, start_frame, duration) in enumerate(cases):
                with self.subTest(options=options):
                    view, context = prepare()
                    path = Path(directory) / f"case-{index}.mp4"
                    summary = export(view, path, **options)
                    self.assertEqual(summary.frames, count)
                    self.assert_movie(path, count, start_frame=start_frame, duration=duration)
                    self.assertEqual(context, {"module": 1, "construct": 1, "completed": 1})
                    view.close()

    def test_cancellation_bad_encoder_and_limits_preserve_existing_destination(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "existing.mp4"
            path.write_bytes(b"previous user movie")
            token = ExportCancellation()
            token.cancel()
            cases = [
                {"cancellation": token},
                {"ffmpeg": str(Path(directory) / "missing-ffmpeg")},
                {"max_frames": 2},
            ]
            for options in cases:
                with self.subTest(options=options):
                    view, _ = prepare()
                    with self.assertRaises(RuntimeError):
                        export(view, path, overwrite=True, **options)
                    self.assertEqual(path.read_bytes(), b"previous user movie")
                    self.assertEqual([entry.name for entry in Path(directory).iterdir()], ["existing.mp4"])
                    view.close()
            view, _ = prepare()
            with self.assertRaises(RuntimeError):
                export(view, path)
            self.assertEqual(path.read_bytes(), b"previous user movie")
            view, _ = prepare()
            export(view, path, overwrite=True, frames=1)
            self.assert_movie(path, 1)

    def test_unrecorded_callbacks_remain_rejected_before_export(self):
        source = '''from noon import *
class CallbackScene(Scene):
    def construct(self):
        marker = Square(0.8)
        self.add(marker)
        marker.add_updater(lambda obj, dt: obj.shift(dt * RIGHT))
        self.wait(0.1)
'''
        scene = asyncio.run(run_source(source, retain_history=True))
        with self.assertRaises((RuntimeError, NotImplementedError)):
            prepare_view(scene)

    def test_cli_uses_native_export_and_validates_before_executing_source(self):
        environment = dict(os.environ, PYTHONPATH=str(Path(noon_native.__file__).resolve().parent))
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source = root / "lesson.py"
            source.write_text(SOURCE.replace('context["module"] += 1', 'print("MODULE_ONCE")')
                              .replace('context["construct"] += 1', 'print("CONSTRUCT_ONCE")')
                              .replace('context["completed"] += 1', 'print("COMPLETED_ONCE")'))
            path = root / "lesson.mp4"
            command = [sys.executable, "-m", "noon_native", str(source), "--mp4", str(path),
                       "--fps", "30000/1001", "--width", str(WIDTH), "--height", str(HEIGHT)]
            result = subprocess.run(command, check=True, capture_output=True, text=True, cwd=root, env=environment)
            for marker in ("MODULE_ONCE", "CONSTRUCT_ONCE", "COMPLETED_ONCE"):
                self.assertEqual(result.stdout.count(marker), 1)
            self.assert_movie(path, 22, duration=0.73)
            for extra in (["--fps", "0"], ["--headless"], ["--width", "3"], ["--frames", "0"]):
                result = subprocess.run(command + extra, capture_output=True, text=True, cwd=root, env=environment)
                self.assertNotEqual(result.returncode, 0)
                self.assertNotIn("MODULE_ONCE", result.stdout)
            result = subprocess.run([sys.executable, "-m", "noon_native", "--version"],
                                    check=True, capture_output=True, text=True, cwd=root, env=environment)
            self.assertTrue(result.stdout.startswith("Noon "))


if __name__ == "__main__":
    unittest.main()
