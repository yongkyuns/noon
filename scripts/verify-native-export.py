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


def verify(root: Path) -> list[dict]:
    reports = []
    rows = list(csv.DictReader((root / 'cases.tsv').open(), delimiter='\t'))
    assert len(rows) == 3, 'incomplete proof corpus'
    for row in rows:
        name = row['name']
        p, q, width, height, count = (int(row[k]) for k in ('p', 'q', 'width', 'height', 'frames'))
        video = root / f'{name}.mp4'
        probe = json.loads(run(['ffprobe', '-v', 'error', '-select_streams', 'v:0', '-count_frames',
            '-show_streams', '-show_frames', '-of', 'json', str(video)]))
        (stream,) = probe['streams']
        assert stream['codec_name'] == 'h264'
        assert (stream['width'], stream['height']) == (width, height)
        assert stream['pix_fmt'] == 'yuv420p'
        assert Fraction(stream['r_frame_rate']) == Fraction(p, q)
        assert Fraction(stream['avg_frame_rate']) == Fraction(p, q)
        assert Fraction(stream['time_base']) == Fraction(1, p)
        assert int(stream['nb_read_frames']) == count
        assert int(stream['start_pts']) == 0
        assert int(stream['duration_ts']) == count * q
        assert [int(f['pts']) for f in probe['frames']] == [i * q for i in range(count)]
        assert stream['color_range'] == 'tv'
        assert stream['color_space'] == 'bt709'
        assert stream['color_primaries'] == 'bt709'
        assert stream['color_transfer'] == 'iec61966-2-1'
        raw_path = root / f'{name}.rgba'
        reference = raw_path.read_bytes()
        frame_bytes = width * height * 4
        assert len(reference) == count * frame_bytes
        decoded = run(['ffmpeg', '-v', 'error', '-nostdin', '-i', str(video), '-frames:v', str(count + 1),
            '-vf', 'scale=in_range=limited:out_range=full:in_color_matrix=bt709,format=rgba',
            '-fps_mode', 'passthrough', '-f', 'rawvideo', 'pipe:1'])
        assert len(decoded) == len(reference)
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
            assert active_count > 0, f'{name}: empty oracle frame {i}'
            active_error = active_sum / active_count
            assert error <= 3.0, (name, i, 'RGBA mean error', error)
            assert active_error <= 25.0, (name, i, 'foreground RGB mean error', active_error)
            errors.append(error)
            active_errors.append(active_error)
            hashes.append(hashlib.sha256(expected).hexdigest())
        png_identical = None
        if row['png'] == 'true':
            directory = root / f'{name}-png' / 'frames'
            files = sorted(directory.glob('frame-*.png'))
            assert [f.name for f in files] == [f'frame-{i:010}.png' for i in range(count)]
            pixels = run(['ffmpeg', '-v', 'error', '-framerate', f'{p}/{q}', '-start_number', '0',
                '-i', str(directory / 'frame-%010d.png'), '-frames:v', str(count + 1),
                '-fps_mode', 'passthrough', '-f', 'rawvideo', '-pix_fmt', 'rgba', 'pipe:1'])
            assert pixels == reference, 'PNG sequence is not byte-identical to the independent capture'
            manifest = (directory / 'timing.tsv').read_text().splitlines()
            assert manifest[0] == f'# noon-rgba8-v1 fps={p}/{q} width={width} height={height}'
            assert manifest[-1] == f'# complete frames={count}'
            samples = list(csv.DictReader(manifest[1:-1], delimiter='\t'))
            assert len(samples) == count
            for i, sample in enumerate(samples):
                assert int(sample['pts']) == i == int(sample['source_index'])
                assert abs(float(sample['requested_time']) - float(Fraction(i * q, p))) < 1e-14
                assert sample['published_time'] == sample['requested_time']
                assert sample['held'] == 'false'
            assert not (directory.parent / '.incomplete').exists()
            png_identical = True
        report = {'name': name, 'frames': count, 'fps': f'{p}/{q}', 'time_base': f'1/{p}',
            'duration_ticks': count * q, 'decoded_all_frames': True, 'max_rgba_mean_error': max(errors),
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
