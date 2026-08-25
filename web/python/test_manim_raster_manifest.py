import ast
import json
import pathlib
import unittest


REPO_ROOT = pathlib.Path(__file__).resolve().parents[2]
MANIFEST_PATH = REPO_ROOT / "parity" / "manim-v0.21" / "manifest.json"


class ManimRasterManifestTests(unittest.TestCase):
    def setUp(self) -> None:
        self.manifest = json.loads(MANIFEST_PATH.read_text(encoding="utf-8"))
        self.source_path = REPO_ROOT / self.manifest["reference"]["source"]
        self.source = self.source_path.read_text(encoding="utf-8")
        self.tree = ast.parse(self.source)

    def test_reference_profile_is_pinned(self) -> None:
        reference = self.manifest["reference"]
        self.assertEqual(reference["version"], "0.21.0")
        self.assertEqual(reference["renderer"], "cairo")
        self.assertEqual((reference["pixel_width"], reference["pixel_height"]), (960, 540))
        self.assertEqual(reference["frame_rate"], 30)

    def test_canonical_source_uses_real_manim_only(self) -> None:
        self.assertIn("from manim import *", self.source)
        self.assertNotIn("from noon import *", self.source)
        imports = [node for node in self.tree.body if isinstance(node, ast.ImportFrom)]
        self.assertTrue(any(node.module == "manim" for node in imports))

    def test_every_fixture_selects_a_scene_class(self) -> None:
        classes = {
            node.name
            for node in self.tree.body
            if isinstance(node, ast.ClassDef)
        }
        fixtures = self.manifest["fixtures"]
        self.assertGreaterEqual(len(fixtures), 4)
        self.assertEqual(len({fixture["id"] for fixture in fixtures}), len(fixtures))
        for fixture in fixtures:
            self.assertIn(fixture["scene"], classes)
            self.assertGreater(float(fixture["expected_duration"]), 0.0)

    def test_samples_cover_animation_span(self) -> None:
        fractions = self.manifest["sample_fractions"]
        self.assertEqual(fractions[0], 0.0)
        self.assertEqual(fractions[-1], 1.0)
        self.assertIn(0.25, fractions)
        self.assertIn(0.5, fractions)
        self.assertIn(0.75, fractions)
        self.assertEqual(fractions, sorted(set(fractions)))

    def test_noon_adaptation_changes_only_import_before_selection_wrapper(self) -> None:
        adapted = self.source.replace("from manim import *", "from noon import *", 1)
        restored = adapted.replace("from noon import *", "from manim import *", 1)
        self.assertEqual(restored, self.source)


if __name__ == "__main__":
    unittest.main()
