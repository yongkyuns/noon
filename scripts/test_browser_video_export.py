"""Video playback and export regressions for the built browser package."""

import json
from pathlib import Path
import struct
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]
FIXTURE = ROOT / "crates/noon-export/tests/fixtures/video-pts-and-click.ffmpeg"
WIDTH, HEIGHT = 128, 72
FRAME_STEP = 1001 / 30_000
SOURCE = """\
from noon import Circle, GREEN, RED, RIGHT, Scene, VideoSource, linear

result = Scene()
clip = await VideoSource.from_upload()
result.video(clip, start_time=0.1, source_start=0.125, source_end=1.25, height=8)
circle = Circle(radius=0.5, color=RED, fill_opacity=1, stroke_width=0)
result.add(circle)
"""

HISTORY_SOURCE = SOURCE + """\
result.play(circle.animate.shift(RIGHT * 0.25), run_time=0.4, rate_func=linear)
circle.set_fill(GREEN, opacity=1)
result.wait(0.925)
"""


def decode(path, *options):
    return subprocess.check_output(["ffmpeg", "-v", "error", "-nostdin", "-i", str(path), *options, "pipe:1"])


class BrowserVideoExportTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        temporary = tempfile.TemporaryDirectory()
        cls.addClassCleanup(temporary.cleanup)
        cls.asset = Path(temporary.name) / "source.mp4"
        subprocess.run(["ffmpeg", *FIXTURE.read_text().splitlines(), str(cls.asset)], check=True)

    def test_scrub_resume_and_replace_during_decode(self):
        subprocess.run(["node", "scripts/playground-video-lifecycle.mjs", str(self.asset)],
                       cwd=ROOT, check=True)

    def test_cropped_export_uses_the_same_composited_samples_and_trimmed_sound(self):
        self.check_full_and_cropped(SOURCE, 1.225, 37, "immutable")

    def test_completed_play_and_wait_export_with_audio(self):
        self.check_full_and_cropped(HISTORY_SOURCE, 1.325, 40, "completed_history")

    def check_full_and_cropped(self, script, end, frame_count, kind):
        with tempfile.TemporaryDirectory() as directory:
            directory = Path(directory)
            source = directory / "scene.py"
            source.write_text(script)
            captures = []
            for start in (0, 4):
                output, capture = directory / f"movie-{start}.mp4", directory / f"capture-{start}"
                with self.subTest(start_frame=start):
                    report = json.loads(subprocess.check_output([
                        "node", "scripts/export-python-scene.mjs", str(source), str(capture),
                        "--video", str(self.asset), "--mp4", str(output), "--width", str(WIDTH),
                        "--height", str(HEIGHT), "--fps", "30000/1001", "--start-frame", str(start),
                    ], cwd=ROOT))
                    manifest = json.loads((capture / "capture.json").read_text())
                    self.assertEqual(manifest["source"]["kind"], kind)
                    if kind == "completed_history":
                        revisions = {sample["publication"]["execution_revision"] for sample in manifest["samples"]}
                        self.assertGreater(len(revisions), 1)
                    self.assertEqual(report["native"]["frames"], frame_count - start)
                    self.assertAlmostEqual(report["native"]["end_time"], end)
                    self.verify_movie(output, start, frame_count, kind == "completed_history")
                    captures.append((capture / "frames.rgba").read_bytes())
            self.assertEqual(len(captures), 2)
            self.assertEqual(captures[0][4 * WIDTH * HEIGHT * 4:], captures[1])

    def verify_movie(self, output, start, frame_count, changed_style):
        pcm = decode(output, "-vn", "-ac", "1", "-ar", "48000", "-f", "f32le")
        click = next(index / 48_000 for index, (sample,) in enumerate(struct.iter_unpack("<f", pcm))
                     if abs(sample) > 0.12)
        self.assertAlmostEqual(click, 0.225 - start * FRAME_STEP, delta=0.012)
        video = decode(output, "-an", "-fps_mode", "passthrough", "-pix_fmt", "rgb24", "-f", "rawvideo")
        frame_size = WIDTH * HEIGHT * 3
        self.assertEqual(len(video), (frame_count - start) * frame_size)

        def pixel(frame, x, y):
            offset = frame * frame_size + (y * WIDTH + x) * 3
            return video[offset:offset + 3]

        flash = next(index for index in range(frame_count - start) if min(pixel(index, 96, 40)) > 215)
        self.assertAlmostEqual(flash * FRAME_STEP, click, delta=FRAME_STEP)
        self.assertEqual([min(pixel(flash, 5 + 11 * bit, 4)) > 128 for bit in range(5)],
                         [True, True, False, False, False])
        red, green, blue = pixel(flash, 64, 36)
        self.assertGreater(red, max(green, blue) + 80, "the foreground circle must cover the video")
        if changed_style:
            red, green, blue = pixel(frame_count - start - 1, 66, 36)
            self.assertGreater(green, max(red, blue) + 25, "the trailing wait must retain the final fill")
        if start == 0:
            self.assertLess(max(pixel(0, 96, 40)), 20, "the video must be hidden before clip entry")


if __name__ == "__main__":
    unittest.main()
