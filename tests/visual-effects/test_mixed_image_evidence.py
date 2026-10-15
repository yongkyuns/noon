"""Synthetic validator regressions; none of these are GPU execution evidence."""
import tempfile
import unittest
from pathlib import Path

from mixed_image_evidence import mixed_image_cases, mixed_image_evidence_error


class MixedImageEvidenceTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        (self.root / "PASS.txt").write_text("CACHE_MIXED_IMAGE_PROBE_PASS: synthetic fixture\n")
        (self.root / "profile.txt").write_text(
            "samples=4 transfer=Direct\nsampling=Nearest,Linear,Bicubic\n")
        for name, size, equal in mixed_image_cases():
            changed = "-s1-" in name or "-rebuilt-" in name or "-stale-resource" in name
            shift = 41 if changed else 0
            # Deterministic nonuniform bytes, deliberately not rendered images.
            expected = bytes((i + shift) % 251 for i in range(1024))
            expected = (expected * ((size + 1023) // 1024))[:size]
            actual = expected
            if "-stale-resource" in name:
                actual = bytes(i % 251 for i in range(1024))
                actual = (actual * ((size + 1023) // 1024))[:size]
            elif not equal:
                actual = bytes([expected[0] ^ 1]) + expected[1:]
            self.write(name, expected, actual)

    def write(self, name, expected, actual):
        (self.root / f"{name}-expected.rgba").write_bytes(expected)
        (self.root / f"{name}-actual.rgba").write_bytes(actual)
        (self.root / f"{name}-diff.mask").write_bytes(bytes(
            0 if expected[i:i+4] == actual[i:i+4] else 255
            for i in range(0, len(expected), 4)))

    def pair(self, name, value):
        self.write(name, value, value)

    def test_complete_fixture_and_frozen_inventory(self):
        cases = list(mixed_image_cases())
        self.assertEqual(len(cases), 138)
        self.assertEqual(sum(equal for _, _, equal in cases), 114)
        self.assertIsNone(mixed_image_evidence_error(self.root))

    def test_missing_truncated_and_wrong_mask_are_rejected(self):
        path = self.root / "i5-rebuilt-samples-diff.mask"
        original = path.read_bytes()
        for value in (b"", b"\xff" + original[1:]):
            path.write_bytes(value)
            self.assertIsNotNone(mixed_image_evidence_error(self.root))
        path.unlink()
        self.assertIsNotNone(mixed_image_evidence_error(self.root))

    def test_completion_and_sampling_profile_are_required(self):
        marker = self.root / "PASS.txt"
        marker.write_text("CACHE_GEOMETRY_PROBE_PASS: wrong probe")
        self.assertIsNotNone(mixed_image_evidence_error(self.root))
        marker.write_text("CACHE_MIXED_IMAGE_PROBE_PASS: fixture")
        (self.root / "profile.txt").write_text("samples=4 transfer=Direct")
        self.assertIsNotNone(mixed_image_evidence_error(self.root))

    def test_ineffective_missing_image_control_is_rejected(self):
        name = "i0-missing-image"
        original = (self.root / f"{name}-expected.rgba").read_bytes()
        self.pair(name, original)
        self.assertIsNotNone(mixed_image_evidence_error(self.root))

    def test_matching_wrong_fresh_reference_is_rejected(self):
        name = "i0-s1-fresh"
        original = (self.root / f"{name}-expected.rgba").read_bytes()
        self.pair(name, bytes([original[0] ^ 1]) + original[1:])
        self.assertIsNotNone(mixed_image_evidence_error(self.root))

    def test_self_consistent_history_drift_is_rejected(self):
        for kind in ("ordinary", "fresh", "resolved", "samples"):
            name = f"i0-s2-{kind}"
            original = (self.root / f"{name}-expected.rgba").read_bytes()
            self.pair(name, bytes([original[0] ^ 1]) + original[1:])
        self.assertIsNotNone(mixed_image_evidence_error(self.root))

    def test_disconnected_stale_resource_control_is_rejected(self):
        name = "i0-stale-resource"
        expected = (self.root / f"{name}-expected.rgba").read_bytes()
        actual = (self.root / f"{name}-actual.rgba").read_bytes()
        self.write(name, expected, bytes([actual[0] ^ 1]) + actual[1:])
        self.assertIsNotNone(mixed_image_evidence_error(self.root))

    def test_no_foreground_change_or_flat_samples_are_rejected(self):
        for kind in ("ordinary", "fresh", "resolved", "samples"):
            original = (self.root / f"i0-s0-{kind}-actual.rgba").read_bytes()
            self.pair(f"i0-s1-{kind}", original)
        self.assertIsNotNone(mixed_image_evidence_error(self.root))


if __name__ == "__main__":
    unittest.main()
