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
    def test_vector_space_lts_has_direct_and_worker_pairs(self):
        manifest = json.loads(MANIFEST.read_text(encoding="utf-8"))
        fixtures = {
            fixture["id"]: fixture
            for fixture in manifest["fixtures"]
            if fixture["id"] in {"vector-space-lts-direct", "vector-space-lts-worker"}
        }
        self.assertEqual(set(fixtures), {"vector-space-lts-direct", "vector-space-lts-worker"})
        direct = fixtures["vector-space-lts-direct"]
        worker = fixtures["vector-space-lts-worker"]
        self.assertEqual(direct["scene"], "VectorSpaceLTS")
        self.assertEqual(worker["scene"], direct["scene"])
        self.assertEqual(direct["source"], worker["source"])
        self.assertEqual(direct["direct_factory"], "createDirectVectorSpaceSmokeRenderer")
        self.assertEqual(direct["expected_duration"], 3.0)
        self.assertEqual(worker["expected_duration"], 3.0)
        self.assertEqual(direct["sample_times"], [0.0, 1.5, 2.966666666666667])
        self.assertEqual(worker["sample_times"], direct["sample_times"])
        self.assertNotIn("tolerance", direct)
        self.assertNotIn("tolerance", worker)
        source = (ROOT / direct["source"]).read_text(encoding="utf-8")
        self.assertIn("from manim import *", source)
        self.assertIn("class VectorSpaceLTS(LinearTransformationScene)", source)
        self.assertIn("self.apply_matrix([[0.0, 1.0], [1.0, 0.0]])", source)

    def test_spatial_mesh_fixture_is_a_direct_typed_rust_wasm_pair(self):
        manifest = json.loads(MANIFEST.read_text(encoding="utf-8"))
        fixtures = [
            fixture for fixture in manifest["fixtures"]
            if fixture["id"] == "spatial-mesh-depth"
        ]
        self.assertEqual(len(fixtures), 1)
        fixture = fixtures[0]
        self.assertEqual(fixture["scene"], "SpatialMeshDepthOracle")
        self.assertEqual(fixture["direct_factory"], "createDirectSpatialMeshSmokeRenderer")
        self.assertEqual(fixture["expected_duration"], 2.0)
        self.assertEqual(fixture["sample_times"], [0.0, 0.5, 1.0, 1.5, 1.9666666666666666])
        self.assertIn(fixture["expected_duration"] / 2, fixture["sample_times"])
        source = (ROOT / fixture["source"]).read_text(encoding="utf-8")
        raster_driver = (ROOT / "scripts/manim-raster-differential.mjs").read_text(
            encoding="utf-8"
        )
        for required in (
            "class SpatialMeshDepthOracle(ThreeDScene)",
            "focal_distance=5",
            "zoom=4 / (5 * np.tan(0.5))",
            "frame_center=RIGHT * 0.25",
            "self.red_mesh.animate.shift(2.0 * IN)",
            "def noon_oracle_state(self)",
        ):
            self.assertIn(required, source)
        self.assertIn("if (fixture.direct_factory)", raster_driver)
        self.assertIn("direct_typed_execution", raster_driver)

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
