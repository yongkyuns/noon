import json
import math
import unittest
from pathlib import Path


class PublicGalleryManifestTests(unittest.TestCase):
    def test_public_gallery_is_manifest_driven_and_manim_only(self) -> None:
        web_root = Path(__file__).parents[1]
        repo_root = web_root.parent
        main_js = (web_root / "main.js").read_text(encoding="utf-8")
        manifest = json.loads(
            (web_root / "python" / "examples" / "manim_tutorial_manifest.json").read_text(
                encoding="utf-8"
            )
        )
        ready = [entry for entry in manifest["entries"] if entry["status"] == "ready"]

        self.assertTrue(ready)
        self.assertNotIn("const SCENE_EXAMPLES = [", main_js)
        self.assertNotIn("const PATCH_EXAMPLES = [", main_js)
        self.assertIn("loadGalleryManifest", main_js)
        self.assertIn("const SCENE_EXAMPLES = galleryManifest.examples", main_js)

        for entry in ready:
            with self.subTest(entry=entry["id"]):
                self.assertEqual(entry["reuse"], "source-equivalent-manim-v0.21")
                parity_status = entry["parity_status"]
                self.assertIn(parity_status, {"candidate", "parity-qualified"})

                parity_fixture = entry.get("parity_fixture")
                expected_duration = entry.get("expected_duration")
                self.assertTrue(
                    parity_fixture or expected_duration is not None,
                    "runnable exact-source examples need a parity fixture or deterministic duration contract",
                )
                if parity_status == "parity-qualified":
                    self.assertTrue(
                        parity_fixture,
                        "parity-qualified examples require canonical raster/timeline fixtures",
                    )
                if parity_fixture is None:
                    duration = float(expected_duration)
                    self.assertTrue(math.isfinite(duration) and duration >= 0.0)

                self.assertTrue((web_root / entry["path"]).is_file())
                self.assertTrue((web_root / entry["thumbnail"]).is_file())
                upstream_source = entry.get("upstream_source")
                self.assertIsInstance(upstream_source, str)
                self.assertTrue((repo_root / upstream_source).is_file())



if __name__ == "__main__":
    unittest.main()
