#!/usr/bin/env python3
"""Independently decode the bounded native-output proof corpus; no engine imports."""
from __future__ import annotations
import csv
from fractions import Fraction
import hashlib
import json
from pathlib import Path
import subprocess
import sys


def run(args: list[str]) -> bytes:
    result = subprocess.run(args, stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=120, check=False)
    if result.returncode:
        raise RuntimeError(f'{args[0]} failed ({result.returncode}): {result.stderr.decode(errors="replace")}')
    return result.stdout


# These tests are executable validation, not debug-only Python assertions.
# A disabled assertion under -O/-OO would silently certify a corrupted movie.
EXPECTED_CASES = {
    'camera-30': (30, 1, 320, 180, 90, 'true'),
    'camera-2997': (30_000, 1_001, 320, 180, 90, 'false'),
    'camera-5994': (60_000, 1_001, 320, 180, 180, 'false'),
}


class VerificationError(RuntimeError):
    """The proof corpus violates its independently pinned export contract."""


def require(condition: bool, description: object) -> None:
    if not condition:
        raise VerificationError(str(description))


def verify(root: Path) -> list[dict]:
    reports = []
    rows = list(csv.DictReader((root / 'cases.tsv').open(), delimiter='\t'))
    require(len(rows) == len(EXPECTED_CASES) and
            {row['name'] for row in rows} == set(EXPECTED_CASES),
            'missing, duplicated or unexpected export proof cases')
    for row in rows:
        name = row['name']
        p, q, width, height, count = (int(row[k]) for k in ('p', 'q', 'width', 'height', 'frames'))
        require((p, q, width, height, count, row['png']) == EXPECTED_CASES[name],
                f'{name}: proof frame rate, dimensions, count or PNG selection changed')
        video = root / f'{name}.mp4'
        probe = json.loads(run(['ffprobe', '-v', 'error', '-select_streams', 'v:0', '-count_frames',
            '-show_streams', '-show_frames', '-of', 'json', str(video)]))
        (stream,) = probe['streams']
        require(stream['codec_name'] == 'h264', "video proof failed: stream['codec_name'] == 'h264'")
        require((stream['width'], stream['height']) == (width, height), "video proof failed: (stream['width'], stream['height']) == (width, height)")
        require(stream['pix_fmt'] == 'yuv420p', "video proof failed: stream['pix_fmt'] == 'yuv420p'")
        require(Fraction(stream['r_frame_rate']) == Fraction(p, q), "video proof failed: Fraction(stream['r_frame_rate']) == Fraction(p, q)")
        # The final sample may be shorter than the grid interval; average FPS
        # uses exact authored duration, whereas the nominal rate stays p/q.
        duration_ticks = min(count * q, 3 * p)
        require(Fraction(stream['avg_frame_rate']) == Fraction(count * p, duration_ticks),
                'video proof failed: average rate does not match exact terminal duration')
        require(Fraction(stream['time_base']) == Fraction(1, p), "video proof failed: Fraction(stream['time_base']) == Fraction(1, p)")
        require(int(stream['nb_read_frames']) == count, "video proof failed: int(stream['nb_read_frames']) == count")
        require(int(stream['start_pts']) == 0, "video proof failed: int(stream['start_pts']) == 0")
        require(int(stream['duration_ts']) == duration_ticks,
                'video proof failed: encoded duration does not match authored terminal time')
        require([int(f['pts']) for f in probe['frames']] == [i * q for i in range(count)], "video proof failed: [int(f['pts']) for f in probe['frames']] == [i * q for i in range(count)]")
        require(stream['color_range'] == 'tv', "video proof failed: stream['color_range'] == 'tv'")
        require(stream['color_space'] == 'bt709', "video proof failed: stream['color_space'] == 'bt709'")
        require(stream['color_primaries'] == 'bt709', "video proof failed: stream['color_primaries'] == 'bt709'")
        require(stream['color_transfer'] == 'iec61966-2-1', "video proof failed: stream['color_transfer'] == 'iec61966-2-1'")
        raw_path = root / f'{name}.rgba'
        reference = raw_path.read_bytes()
        frame_bytes = width * height * 4
        require(len(reference) == count * frame_bytes, "video proof failed: len(reference) == count * frame_bytes")
        decoded = run(['ffmpeg', '-v', 'error', '-nostdin', '-i', str(video), '-frames:v', str(count + 1),
            '-vf', 'scale=in_range=limited:out_range=full:in_color_matrix=bt709,format=rgba',
            '-fps_mode', 'passthrough', '-f', 'rawvideo', 'pipe:1'])
        require(len(decoded) == len(reference), "video proof failed: len(decoded) == len(reference)")
        errors, active_errors, hashes = [], [], []
        for i in range(count):
            expected = memoryview(reference)[i * frame_bytes:(i + 1) * frame_bytes]
            actual = memoryview(decoded)[i * frame_bytes:(i + 1) * frame_bytes]
            error = sum(abs(a - b) for a, b in zip(expected, actual)) / frame_bytes
            active_sum = active_count = 0
            for j in range(0, frame_bytes, 4):
                if max(expected[j:j + 3]) > 12:
                    active_sum += sum(abs(expected[j + c] - actual[j + c]) for c in range(3))
                    active_count += 3
            require(active_count > 0, f'{name}: empty oracle frame {i}')
            active_error = active_sum / active_count
            require(error <= 3.0, (name, i, 'RGBA mean error', error))
            require(active_error <= 25.0, (name, i, 'foreground RGB mean error', active_error))
            errors.append(error)
            active_errors.append(active_error)
            hashes.append(hashlib.sha256(expected).hexdigest())
        png_identical = None
        if row['png'] == 'true':
            directory = root / f'{name}-png' / 'frames'
            files = sorted(directory.glob('frame-*.png'))
            require([f.name for f in files] == [f'frame-{i:010}.png' for i in range(count)], "video proof failed: [f.name for f in files] == [f'frame-{i:010}.png' for i in range(count)]")
            pixels = run(['ffmpeg', '-v', 'error', '-framerate', f'{p}/{q}', '-start_number', '0',
                '-i', str(directory / 'frame-%010d.png'), '-frames:v', str(count + 1),
                '-fps_mode', 'passthrough', '-f', 'rawvideo', '-pix_fmt', 'rgba', 'pipe:1'])
            require(pixels == reference, 'PNG sequence is not byte-identical to the independent capture')
            manifest = (directory / 'timing.tsv').read_text().splitlines()
            require(manifest[0] == f'# noon-rgba8-v1 fps={p}/{q} width={width} height={height}', "video proof failed: manifest[0] == f'# noon-rgba8-v1 fps={p}/{q} width={width} height={height}'")
            require(manifest[-1] == f'# complete frames={count}', "video proof failed: manifest[-1] == f'# complete frames={count}'")
            samples = list(csv.DictReader(manifest[1:-1], delimiter='\t'))
            require(len(samples) == count, "video proof failed: len(samples) == count")
            for i, sample in enumerate(samples):
                require(int(sample['pts']) == i == int(sample['source_index']), "video proof failed: int(sample['pts']) == i == int(sample['source_index'])")
                require(abs(float(sample['requested_time']) - float(Fraction(i * q, p))) < 1e-14, "video proof failed: abs(float(sample['requested_time']) - float(Fraction(i * q, p))) < 1e-14")
                require(sample['published_time'] == sample['requested_time'], "video proof failed: sample['published_time'] == sample['requested_time']")
                require(sample['held'] == 'false', "video proof failed: sample['held'] == 'false'")
            require(not (directory.parent / '.incomplete').exists(), "video proof failed: not (directory.parent / '.incomplete').exists()")
            png_identical = True
        report = {'name': name, 'frames': count, 'fps': f'{p}/{q}', 'time_base': f'1/{p}',
            'duration_ticks': duration_ticks, 'decoded_all_frames': True, 'max_rgba_mean_error': max(errors),
            'max_foreground_rgb_mean_error': max(active_errors), 'png_byte_identical': png_identical,
            'reference_sha256': hashlib.sha256(reference).hexdigest(), 'frame_sha256': hashes,
            'mp4_sha256': hashlib.sha256(video.read_bytes()).hexdigest()}
        reports.append(report)
        (root / f'{name}.ffprobe.json').write_text(json.dumps(probe, indent=2))
        print(f'{name}: {count} decoded frames, {p}/{q} FPS, exact PTS; max mean error {max(errors):.3f}')
    return reports


if __name__ == '__main__':
    root = Path(sys.argv[1]).resolve()
    reports = verify(root)
    (root / 'verification.json').write_text(json.dumps(reports, indent=2) + '\n')
    # Raw references are bulky; retain their per-frame hashes and PNG equivalent.
    # Only remove them AFTER every case has independently passed.
    for report in reports:
        (root / f'{report["name"]}.rgba').unlink()
