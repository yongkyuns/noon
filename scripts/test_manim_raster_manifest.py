"""Exercise real pinned-Manim source resolution with relocated manifests."""

import copy
import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parent.parent
MANIFEST = ROOT / "parity/manim-v0.21/manifest.json"
ORACLE = ROOT / "scripts/manim-raster-semantic-reference.py"


class RasterManifestTests(unittest.TestCase):
    def test_manifest_location_and_cwd_do_not_change_fixture_sources(self):
        manifest = json.loads(MANIFEST.read_text(encoding="utf-8"))
        matching = [
            fixture for fixture in manifest["fixtures"]
            if fixture["scene"] == "MatchingShapesReordered"
        ]
        self.assertEqual(len(matching), 1)
        manifest["fixtures"] = matching
        original_manifest = MANIFEST.read_bytes()

        with tempfile.TemporaryDirectory(prefix="noon-raster-manifest-") as directory:
            outside = Path(directory)

            def capture(config, manifest_path, cwd, name):
                manifest_path.write_text(json.dumps(config), encoding="utf-8")
                output = outside / f"{name}-frames.json"
                result = subprocess.run(
                    [sys.executable, str(ORACLE), "--manifest", str(manifest_path),
                     "--output", str(output)],
                    cwd=cwd, text=True, capture_output=True, timeout=60,
                )
                self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
                return json.loads(output.read_text(encoding="utf-8"))

            # Keep the original manifest depth for the backward-compatible case.
            with tempfile.NamedTemporaryFile(
                dir=MANIFEST.parent, prefix=".manifest-test-", suffix=".json"
            ) as canonical:
                expected = capture(manifest, Path(canonical.name), ROOT, "canonical")

            relocated = capture(manifest, outside / "selected.json", outside, "relocated")
            self.assertEqual(relocated, expected)

            fallback = copy.deepcopy(manifest)
            fallback["reference"]["source"] = fallback["fixtures"][0].pop("source")
            self.assertEqual(
                capture(fallback, outside / "fallback.json", outside, "fallback"),
                expected,
            )

            fixture = expected["fixtures"][0]
            self.assertEqual(expected["manim_version"], "0.21.0")
            self.assertAlmostEqual(fixture["logical_duration"], 2.2)
            times = [frame["time"] for frame in fixture["frames"]]
            for time in (0.0, 0.5, 1.0, 1.5, 2.0, 2.1):
                self.assertTrue(any(abs(actual - time) < 1e-9 for actual in times), time)

        self.assertEqual(MANIFEST.read_bytes(), original_manifest)


if __name__ == "__main__":
    unittest.main()
