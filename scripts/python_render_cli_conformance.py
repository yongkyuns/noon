#!/usr/bin/env python3
"""Actual native CLI/media proof; no mocked resolver, renderer or scene state."""
from __future__ import annotations
import asyncio
from fractions import Fraction
import json
import os
from pathlib import Path
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / 'build/python'))
from noon import resolve_render_options
from noon_native import export_source


def require(condition, message):
    if not condition:
        raise RuntimeError(message)


def command(args, *, success=True, environment=None):
    result = subprocess.run(args, capture_output=True, timeout=120, env=environment)
    require((result.returncode == 0) == success,
            f'command {args!r}: {result.stdout.decode(errors="replace")}\n{result.stderr.decode(errors="replace")}')
    return result.stdout if success else result.stderr


def main():
    output = ROOT / 'artifacts/python-render-cli'
    output.mkdir(parents=True, exist_ok=False)
    environment = dict(os.environ, PYTHONPATH=str(ROOT/'build/python'))
    value = resolve_render_options()
    require((value.width, value.height, value.fps) == (1920, 1080, (60, 1)), 'default render profile')
    source = output / 'scenes.py'
    source.write_text("""from noon import Scene, Square
class Wrong(Scene):
    def construct(self): raise RuntimeError("unselected scene must not run")
class Demo(Scene):
    def construct(self):
        self.add(Square(1).set_fill("#0000ff", opacity=1).set_stroke(width=0))
        self.wait(0.2)
""")
    reports = []
    for rate_text, rate, count in [('60', Fraction(60), 12),
                                  ('29.97', Fraction(2997, 100), 6),
                                  ('60000/1001', Fraction(60000, 1001), 12)]:
        movie = output / f'cli-{rate.numerator}-{rate.denominator}.mp4'
        args = [sys.executable, '-m', 'noon', 'render', '-ql', str(source), 'Demo',
                '-r', '64,32', '--frame_rate', rate_text, '-o', str(movie),
                '--fallback-adapter', '--max-frames', '64']
        command(args, environment=environment)
        probe = json.loads(command(['ffprobe', '-v', 'error', '-show_streams', '-show_frames', '-of', 'json', str(movie)]))
        stream = probe['streams'][0]
        require(stream['codec_name'] == 'h264', 'unexpected codec')
        require((stream['width'], stream['height']) == (64, 32), 'resolution override ignored')
        actual = [Fraction(int(frame['pts'])) * Fraction(stream['time_base']) for frame in probe['frames']]
        require(actual == [Fraction(i) / rate for i in range(count)], 'CLI output PTS/frame count changed')
        raw = command(['ffmpeg', '-v', 'error', '-i', str(movie), '-vsync', '0',
                       '-f', 'rawvideo', '-pix_fmt', 'rgba', 'pipe:1'])
        require(len(raw) == count * 64 * 32 * 4, 'CLI decoded frame count/dimensions changed')
        require(any(raw[i] > raw[i-2] + 20 for i in range(2, len(raw), 4)), 'expected blue scene missing')
        reports.append({'fps': str(rate), 'frames': count, 'every_pts_checked': True})
    cli_png = output / 'cli-png'
    command([sys.executable, '-m', 'noon_native', str(source), 'Demo', '-r', '65,33',
             '--fps', '60000/1001', '--format', 'png', '--output_file', str(cli_png),
             '--fallback-adapter', '--max-frames', '64'], environment=environment)
    api_png = output / 'api-png'
    summary = asyncio.run(export_source(source.read_text(), api_png, filename=str(source), scene_name='Demo',
                                         quality='low_quality', resolution=(65, 33), frame_rate='60000/1001',
                                         format='png', max_frames=64, fallback=True))
    require(summary['frames'] == 12, 'API does not use the CLI frame-rate contract')
    pixels = []
    for directory in (cli_png, api_png):
        names = sorted(p.name for p in (directory/'frames').glob('frame-*.png'))
        require(names == [f'frame-{i:010}.png' for i in range(12)], 'PNG numbering/count')
        raw = command(['ffmpeg', '-v', 'error', '-start_number', '0', '-i', str(directory/'frames/frame-%010d.png'),
                       '-vsync', '0', '-f', 'rawvideo', '-pix_fmt', 'rgba', 'pipe:1'])
        require(len(raw) == 12 * 65 * 33 * 4, 'PNG dimensions were changed')
        pixels.append(raw)
    require(pixels[0] == pixels[1], 'CLI/API PNG pixels differ')
    forbidden = output/'invalid.mp4'
    for bad, diagnostic in ((['--fps', 'nan'], 'frame rate'),
                            (['--format', 'gif'], 'unsupported render format'),
                            (['-n', '4'], '-n is not supported')):
        error = command([sys.executable, '-m', 'noon', str(output/'nonexistent.py'), 'Demo',
                 '-o', str(forbidden), *bad], environment=environment, success=False)
        require(diagnostic in error.decode(errors='replace'), 'bad options were not diagnosed before file access')
        require(not forbidden.exists(), 'invalid options published output')
    (output/'report.json').write_text(json.dumps({'movies': reports, 'png_api_cli_identical': True}, indent=2)+'\n')
    print('native CLI: 3 complete MP4/PTS proofs, odd-size CLI/API PNG identity')


if __name__ == '__main__':
    main()
