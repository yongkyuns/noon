#!/usr/bin/env python3
"""Small mock-decoder fixtures test proof invariants without a GPU or FFmpeg."""
from __future__ import annotations

import importlib.util
import json
from pathlib import Path
from tempfile import TemporaryDirectory
from unittest import TestCase, main, mock

VERIFY = Path(__file__).with_name('verify-native-export.py')
SPEC = importlib.util.spec_from_file_location('noon_native_export_verify', VERIFY)
verifier = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(verifier)


class ProofVerifierTests(TestCase):
    def scenario(self, corrupt: str | None = None):
        with TemporaryDirectory() as temporary:
            root = Path(temporary)
            record = {
                'name': 'mini', 'p': '30', 'q': '1', 'width': '2', 'height': '2',
                'frames': '2', 'png': 'true',
            }
            if corrupt == 'case_name':
                record['name'] = 'unknown'
            if corrupt == 'case_fps':
                record['p'] = '60'
            columns = 'name\tp\tq\twidth\theight\tframes\tpng'
            (root / 'cases.tsv').write_text(
                columns + '\n' + '\t'.join(record[key] for key in columns.split('\t')) + '\n'
            )
            (root / 'mini.mp4').write_bytes(b'opaque encoded fixture')
            raw = bytes([22, 43, 180, 255]) * 4 + bytes([42, 63, 200, 255]) * 4
            if corrupt == 'raw_length':
                raw = raw[:-4]
            if corrupt == 'empty_foreground':
                raw = bytes([0, 0, 0, 255]) * 8
            (root / 'mini.rgba').write_bytes(raw)
            decoded = raw
            if corrupt == 'decoded_length':
                decoded = decoded[:-4]
            if corrupt == 'decoded_pixels':
                decoded = bytes([0, 0, 0, 255]) * 8
            png_decoded = raw if corrupt != 'png_pixels' else bytes([0] * len(raw))
            stream = {
                'codec_name': 'h264', 'width': 2, 'height': 2,
                'pix_fmt': 'yuv420p', 'r_frame_rate': '30/1',
                'avg_frame_rate': '30/1', 'time_base': '1/30',
                'nb_read_frames': '2', 'start_pts': '0',
                'duration_ts': '2', 'color_range': 'tv',
                'color_space': 'bt709', 'color_primaries': 'bt709',
                'color_transfer': 'iec61966-2-1',
            }
            for variant, key, new in [
                ('codec', 'codec_name', 'vp9'),
                ('dimensions', 'width', 4),
                ('pixel_format', 'pix_fmt', 'yuv444p'),
                ('real_rate', 'r_frame_rate', '29/1'),
                ('average_rate', 'avg_frame_rate', '29/1'),
                ('time_base', 'time_base', '1/1000'),
                ('frame_count', 'nb_read_frames', '1'),
                ('start_pts', 'start_pts', '1'),
                ('duration', 'duration_ts', '3'),
                ('range', 'color_range', 'pc'),
                ('matrix', 'color_space', 'smpte170m'),
                ('primaries', 'color_primaries', 'smpte170m'),
                ('transfer', 'color_transfer', 'bt709'),
            ]:
                if corrupt == variant:
                    stream[key] = new
            pts = [{'pts': 0}, {'pts': 1}]
            if corrupt == 'pts':
                pts[1]['pts'] = 2
            probe = {'streams': [stream], 'frames': pts}
            image_dir = root / 'mini-png' / 'frames'
            image_dir.mkdir(parents=True)
            for i in range(2):
                (image_dir / f'frame-{i:010}.png').write_bytes(b'encoded image')
            if corrupt == 'png_files':
                (image_dir / 'frame-0000000001.png').unlink()
            header = '# noon-rgba8-v1 fps=30/1 width=2 height=2'
            last = '# complete frames=2'
            samples = [
                'pts\tsource_index\trequested_time\tpublished_time\theld',
                '0\t0\t0\t0\tfalse',
                '1\t1\t0.03333333333333333\t0.03333333333333333\tfalse',
            ]
            if corrupt == 'manifest_header':
                header += ' wrong'
            if corrupt == 'manifest_complete':
                last = '# complete frames=1'
            if corrupt == 'manifest_rows':
                samples.pop()
            if corrupt == 'manifest_pts':
                samples[2] = samples[2].replace('1\t1\t', '1\t0\t')
            if corrupt == 'manifest_requested':
                samples[2] = samples[2].replace('0.03333333333333333', '0.1', 1)
            if corrupt == 'manifest_published':
                samples[2] = samples[2].replace(
                    '0.03333333333333333\tfalse', '0\tfalse'
                )
            if corrupt == 'manifest_hold':
                samples[2] = samples[2].replace('false', 'true')
            (image_dir / 'timing.tsv').write_text(
                '\n'.join([header, *samples, last]) + '\n'
            )
            if corrupt == 'incomplete':
                (image_dir.parent / '.incomplete').mkdir()

            def fake_decode(args):
                if args[0] == 'ffprobe':
                    return json.dumps(probe).encode()
                self.assertEqual(args[0], 'ffmpeg')
                return png_decoded if any('frame-%010d.png' in a for a in args) else decoded

            with mock.patch.object(verifier, 'EXPECTED_CASES', {
                'mini': (30, 1, 2, 2, 2, 'true')
            }), mock.patch.object(verifier, 'run', side_effect=fake_decode):
                return verifier.verify(root)

    def test_valid_proof(self):
        reports = self.scenario()
        self.assertEqual(len(reports), 1)
        self.assertTrue(reports[0]['decoded_all_frames'])
        self.assertTrue(reports[0]['png_byte_identical'])

    def test_enforced_invariants_including_optimized_python(self):
        corruptions = [
            'case_name', 'case_fps', 'codec', 'dimensions', 'pixel_format',
            'real_rate', 'average_rate', 'time_base', 'frame_count',
            'start_pts', 'duration', 'pts', 'range', 'matrix',
            'primaries', 'transfer', 'raw_length', 'decoded_length',
            'empty_foreground', 'decoded_pixels', 'png_files', 'png_pixels',
            'manifest_header', 'manifest_complete', 'manifest_rows',
            'manifest_pts', 'manifest_requested', 'manifest_published',
            'manifest_hold', 'incomplete',
        ]
        for corruption in corruptions:
            with self.subTest(corruption=corruption):
                with self.assertRaises(verifier.VerificationError):
                    self.scenario(corruption)


if __name__ == '__main__':
    main()
