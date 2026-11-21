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
from noon import Circle, RED, Scene, VideoSource

result = Scene()
clip = await VideoSource.from_upload()
result.video(clip, start_time=0.1, source_start=0.125, source_end=1.25, height=8)
result.add(Circle(radius=0.5, color=RED, fill_opacity=1, stroke_width=0))
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
        with tempfile.TemporaryDirectory() as directory:
            directory = Path(directory)
            source = directory / "scene.py"
            source.write_text(SOURCE)
            captures = []
            for start in (0, 4):
                output, capture = directory / f"movie-{start}.mp4", directory / f"capture-{start}"
                with self.subTest(start_frame=start):
                    report = json.loads(subprocess.check_output([
                        "node", "scripts/export-python-scene.mjs", str(source), str(capture),
                        "--video", str(self.asset), "--mp4", str(output), "--width", str(WIDTH),
                        "--height", str(HEIGHT), "--fps", "30000/1001", "--start-frame", str(start),
                    ], cwd=ROOT))
                    self.assertEqual(report["native"]["frames"], 37 - start)
                    self.assertAlmostEqual(report["native"]["end_time"], 1.225)
                    self.verify_movie(output, start)
                    captures.append((capture / "frames.rgba").read_bytes())
            self.assertEqual(len(captures), 2)
            self.assertEqual(captures[0][4 * WIDTH * HEIGHT * 4:], captures[1])

    def verify_movie(self, output, start):
        pcm = decode(output, "-vn", "-ac", "1", "-ar", "48000", "-f", "f32le")
        click = next(index / 48_000 for index, (sample,) in enumerate(struct.iter_unpack("<f", pcm))
                     if abs(sample) > 0.12)
        self.assertAlmostEqual(click, 0.225 - start * FRAME_STEP, delta=0.012)
        video = decode(output, "-an", "-fps_mode", "passthrough", "-pix_fmt", "rgb24", "-f", "rawvideo")
        frame_size = WIDTH * HEIGHT * 3
        self.assertEqual(len(video), (37 - start) * frame_size)

        def pixel(frame, x, y):
            offset = frame * frame_size + (y * WIDTH + x) * 3
            return video[offset:offset + 3]

        flash = next(index for index in range(37 - start) if min(pixel(index, 96, 40)) > 215)
        self.assertAlmostEqual(flash * FRAME_STEP, click, delta=FRAME_STEP)
        self.assertEqual([min(pixel(flash, 5 + 11 * bit, 4)) > 128 for bit in range(5)],
                         [True, True, False, False, False])
        red, green, blue = pixel(flash, 64, 36)
        self.assertGreater(red, max(green, blue) + 80, "the foreground circle must cover the video")
        if start == 0:
            self.assertLess(max(pixel(0, 96, 40)), 20, "the video must be hidden before clip entry")


if __name__ == "__main__":
    unittest.main()
