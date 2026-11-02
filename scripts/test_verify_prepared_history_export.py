"""Corruption controls for the independently decoded history export oracle."""
import copy
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest

import numpy as np


spec = importlib.util.spec_from_file_location("verify_history", Path(__file__).with_name("verify-prepared-history-export.py"))
verifier = importlib.util.module_from_spec(spec)
spec.loader.exec_module(verifier)


def valid_probe():
    case = verifier.CASES["minimal"]
    return {"video": {"streams": [{"codec_name": "h264", "width": 640, "height": 360,
        "pix_fmt": "yuv420p", "r_frame_rate": "30000/1001", "avg_frame_rate": "30/1",
        "time_base": "1/30000", "nb_read_frames": "90", "start_pts": 0, "duration_ts": 90_000,
        "color_range": "tv", "color_space": "bt709", "color_primaries": "bt709",
        "color_transfer": "iec61966-2-1"}], "format": {"duration": "3.000000"},
        "frames": [{"pts": index * 1001, "pkt_duration": min(1001, 90_000 - index * 1001)}
                   for index in range(90)]},
        "audio": {"streams": [{"codec_name": "aac", "sample_rate": "48000", "channels": 2,
            "time_base": "1/48000", "start_pts": 0, "duration_ts": 144_000}],
            "packets": [{"pts": -1024, "duration": 1024, "side_data_list": [{"skip_samples": 1024}]},
                        {"pts": 0, "duration": case["presented"]}]}}


class CorruptionControls(unittest.TestCase):
    def test_valid_exact_partial_terminal_interval(self):
        result = verifier.verify_timing(valid_probe(), verifier.CASES["minimal"])
        self.assertEqual(result["last_duration_ticks"], 911)

    def test_changed_pts_endpoint_color_or_priming_is_rejected(self):
        mutations = [
            lambda value: value["video"]["frames"][19].update(pts=19 * 1001 + 1),
            lambda value: value["video"]["frames"][-1].update(pkt_duration=1001),
            lambda value: value["video"]["streams"][0].update(duration_ts=90_090),
            lambda value: value["video"]["streams"][0].update(color_space="bt601"),
            lambda value: value["audio"]["streams"][0].update(duration_ts=144_144),
            lambda value: value["audio"]["packets"][0].update(side_data_list=[]),
        ]
        for mutation in mutations:
            with self.subTest(mutation=mutation):
                value = copy.deepcopy(valid_probe())
                mutation(value)
                with self.assertRaises(verifier.VerificationError):
                    verifier.verify_timing(value, verifier.CASES["minimal"])

    def test_missing_shifted_or_regained_pcm_is_rejected(self):
        time = np.arange(4096, dtype=np.float64) / 48_000
        expected = np.stack([.1 * np.sin(2 * np.pi * 430 * time),
                             .1 * np.sin(2 * np.pi * 790 * time)], axis=-1)
        verifier.audio_metrics(expected, expected, .006, "positive")
        for actual in [np.zeros_like(expected), expected * .5, np.roll(expected, 100, axis=0),
                       expected[:, ::-1], expected * float("nan")]:
            with self.assertRaises(verifier.VerificationError):
                verifier.audio_metrics(actual, expected, .006, "corrupt")

    def test_native_circle_shift_or_missing_reveal_is_rejected(self):
        yy, xx = np.ogrid[:360, :640]
        image = np.zeros((360, 640, 4), dtype=np.uint8)
        image[(xx - 320) ** 2 + (yy - 180) ** 2 <= 22.5 ** 2, :3] = 255
        verifier.compare_native_circle(image, image, 30)
        for other in [np.zeros_like(image), np.roll(image, 10, axis=1)]:
            with self.assertRaises(verifier.VerificationError):
                verifier.compare_native_circle(other, image, 30)

    def test_fourier_oracle_rejects_a_prematurely_complete_trace(self):
        stage = {"terms": 7, "oracle": {"keys": [0., .5, 1.],
                 "chain": [[[-2., 0.]], [[2., 0.]], [[-2., 0.]]]}}
        yy, xx = np.ogrid[:360, :640]
        for complete in (False, True):
            image = np.zeros((360, 640, 4), dtype=np.uint8)
            image[179:181, 230:410 if complete else 320, :3] = [92, 208, 179]
            image[(xx + .5 - 320) ** 2 + (yy + .5 - 180) ** 2 <= 2.1 ** 2, :3] = [247, 217, 111]
            if complete:
                with self.assertRaises(verifier.VerificationError):
                    verifier.verify_fourier_picture(image, stage, .25, "premature", False)
            else:
                result = verifier.verify_fourier_picture(image, stage, .25, "partial", False)
                self.assertGreater(result["unrevealed_trace_probes"], 0)

    def test_terminal_picture_rejects_retained_rig_missing_trace_or_caption(self):
        stage = {"terms": 61, "oracle": {"keys": [0., .5, 1.],
                 "chain": [[[-2., 0.]], [[2., 0.]], [[-2., 0.]]]}}
        image = np.zeros((360, 640, 4), dtype=np.uint8)
        image[179:181, 230:410, :3] = [92, 208, 179]
        image[304:308, 250:390, :3] = [92, 208, 179]
        for decoded in (False, True):
            result = verifier.verify_fourier_terminal_picture(image, stage, "finished", decoded)
            self.assertEqual(result["remaining_yellow_rig_pixels"], 0)
            self.assertEqual(result["completed_trace_probes"], 16)
            for name in ("rig", "trace", "missing_caption", "old_gray_caption", "lower_label"):
                with self.subTest(decoded=decoded, corruption=name):
                    changed = image.copy()
                    if name == "rig":
                        changed[179:181, 319:321, :3] = [247, 217, 111]
                    elif name == "trace":
                        changed[179:181, 230:410, :3] = 0
                    else:
                        changed[304:308, 250:390, :3] = 136 if name == "old_gray_caption" else 0
                        if name == "lower_label":
                            changed[329:333, 250:390, :3] = [92, 208, 179]
                    with self.assertRaises(verifier.VerificationError):
                        verifier.verify_fourier_terminal_picture(changed, stage, name, decoded)

    def test_capture_cannot_relabel_runtime_or_terminal_request(self):
        case = verifier.CASES["minimal"]
        source = {"kind": "completed_history", "runtime_identity": "17",
                  "admission": {"scene_revision": "2", "execution_revision": "1", "frame_epoch": "1"}}
        samples = [{"runtime_identity": "17", "requested_time": index * 1001 / 30_000,
                    "published_time": min(index * 1001 / 30_000, 3.),
                    "publication": {"scene_revision": "2", "execution_revision": "1", "frame_epoch": str(index + 1)}}
                   for index in range(91)]
        manifest = {"format": "noon-prepared-capture-v2", "source": source, "width": 640, "height": 360,
                    "audio": True, "source_end": 3., "samples": samples,
                    "frame_options": {"frame_rate": {"numerator": 30_000, "denominator": 1001},
                        "start_frame": 0, "stop": {"kind": "source_end"}, "final_hold_seconds": 0}}
        report = {"capture_source": source, "source_sha256": verifier.MINIMAL_SHA, "browser_errors": []}
        with tempfile.TemporaryDirectory() as directory:
            capture = Path(directory)
            (capture / "capture.json").write_text(json.dumps(manifest))
            verifier.verify_capture(capture, report, case)
            for mutation in [lambda value: value["samples"][20].update(runtime_identity="18"),
                             lambda value: value["samples"][-1].update(requested_time=3.),
                             lambda value: value["samples"][-1].update(published_time=3.003)]:
                changed = copy.deepcopy(manifest)
                mutation(changed)
                (capture / "capture.json").write_text(json.dumps(changed))
                with self.assertRaises(verifier.VerificationError):
                    verifier.verify_capture(capture, report, case)


if __name__ == "__main__":
    unittest.main()
