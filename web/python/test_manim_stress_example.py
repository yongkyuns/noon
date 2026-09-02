import ast
import json
import unittest
from pathlib import Path

WEB_ROOT = Path(__file__).parents[1]
REPO_ROOT = WEB_ROOT.parent
STRESS_MANIFEST_PATH = WEB_ROOT / "python" / "examples" / "manim_stress_manifest.json"
PARITY_MANIFEST_PATH = REPO_ROOT / "parity" / "manim-v0.21" / "manifest.json"


def adapt_manim_source(source: str) -> str:
    marker = "from manim import *"
    if source.count(marker) != 1:
        raise AssertionError("canonical stress source must contain exactly one Manim star import")
    return source.replace(marker, "from noon import *", 1)


class ManimStressExampleTests(unittest.TestCase):
    def test_ready_stress_scene_is_import_only_manim_source(self) -> None:
        manifest = json.loads(STRESS_MANIFEST_PATH.read_text(encoding="utf-8"))
        self.assertEqual(manifest["reference"]["version"], "0.21.0")
        ready = [entry for entry in manifest["entries"] if entry["status"] == "ready"]
        self.assertEqual(len(ready), 1)
        entry = ready[0]
        self.assertEqual(entry["reuse"], "manim-compatible-parity-v0.21")
        self.assertEqual(entry["parity_status"], "candidate")
        self.assertEqual(entry["parity_fixture"], "mixed-object-parity-stress")

        demo_path = WEB_ROOT / entry["path"]
        canonical_path = REPO_ROOT / entry["parity_source"]
        self.assertTrue(demo_path.is_file())
        self.assertTrue(canonical_path.is_file())
        self.assertTrue((WEB_ROOT / entry["thumbnail"]).is_file())

        canonical_source = canonical_path.read_text(encoding="utf-8")
        demo_source = demo_path.read_text(encoding="utf-8")
        self.assertEqual(demo_source, adapt_manim_source(canonical_source))
        compile(ast.parse(canonical_source, filename=str(canonical_path)), str(canonical_path), "exec")
        compile(ast.parse(demo_source, filename=str(demo_path)), str(demo_path), "exec")

        for token in (
            "Text(",
            "Create(",
            "Transform(",
            ".animate.rotate(",
            ".set_color(",
            "FadeIn(",
            "FadeOut(",
        ):
            self.assertIn(token, canonical_source)
        self.assertIn("rows = 6", canonical_source)
        self.assertIn("cols = 12", canonical_source)
        self.assertNotIn("VectorPath", canonical_source)
        self.assertNotIn("context", canonical_source)
        self.assertNotIn("result =", canonical_source)

    def test_stress_scene_is_linked_to_canonical_raster_fixture(self) -> None:
        stress_manifest = json.loads(STRESS_MANIFEST_PATH.read_text(encoding="utf-8"))
        entry = next(item for item in stress_manifest["entries"] if item["status"] == "ready")
        parity_manifest = json.loads(PARITY_MANIFEST_PATH.read_text(encoding="utf-8"))
        fixture = next(
            item
            for item in parity_manifest["fixtures"]
            if item["id"] == entry["parity_fixture"]
        )

        self.assertEqual(fixture["scene"], "MixedObjectParityStress")
        self.assertEqual(fixture["source"], entry["parity_source"])
        self.assertEqual(fixture["expected_duration"], 5.0)


if __name__ == "__main__":
    unittest.main()
