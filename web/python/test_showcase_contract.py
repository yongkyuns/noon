"""Editorial/source contracts live outside the examples and do not substitute for rendering."""
import ast
import json
from pathlib import Path
import unittest

WEB = Path(__file__).resolve().parents[1]
MANIFEST = json.loads((WEB / "python/examples/noon_showcase_manifest.json").read_text())


class ShowcaseSourceContract(unittest.TestCase):
    def test_sources_are_self_contained_animated_lessons(self):
        for entry in MANIFEST["entries"]:
            with self.subTest(example=entry["id"]):
                source = (WEB / entry["path"]).read_text()
                tree = ast.parse(source, filename=entry["path"])
                compile(tree, entry["path"], "exec")
                self.assertFalse(any(isinstance(node, ast.Assert) for node in ast.walk(tree)))
                scenes = [node for node in tree.body if isinstance(node, ast.ClassDef)]
                self.assertEqual(len(scenes), 1)
                self.assertTrue(any(isinstance(base, ast.Name) and base.id == "Scene" for base in scenes[0].bases))
                calls = [node for node in ast.walk(scenes[0]) if isinstance(node, ast.Call)]
                plays = [node for node in calls if isinstance(node.func, ast.Attribute) and isinstance(node.func.value, ast.Name) and node.func.value.id == "self" and node.func.attr == "play"]
                self.assertGreaterEqual(len(plays), 2, "a lesson needs a progression, not just a static API probe")
                self.assertTrue(any(isinstance(node.func, ast.Name) and node.func.id in {"FadeIn", "Write", "Create"} for node in calls))
                self.assertTrue(any(isinstance(node.func, ast.Attribute) and node.func.attr == "wait" for node in calls), "hold a readable state")
                for node in ast.walk(tree):
                    if isinstance(node, ast.ImportFrom):
                        self.assertIn(node.module, {"noon", "math"}, "keep examples runnable in the existing browser authoring environment")
                    self.assertNotIsInstance(node, ast.Import, "new package dependencies require explicit qualification")

    def test_dynamic_scene_is_not_the_unchanged_regression_workload(self):
        entry = next(item for item in MANIFEST["entries"] if item.get("performance"))
        self.assertEqual(len(entry["beats"]), 6)
        self.assertGreater(entry["duration"], 20, "allow viewers to perceive the distinct animation sequences")
        source = (WEB / entry["path"]).read_text()
        self.assertIn("ROWS = 20", source)
        self.assertIn("COLS = 30", source)
        self.assertIn("TRACKS = 24", source)
        self.assertIn("shapes[::3]", source)
        self.assertNotEqual(entry["path"], "python/examples/manim_parity_stress_grid.py")

    def test_preview_destinations_do_not_reuse_illustrated_reference_posters(self):
        posters = [entry["thumbnail"] for entry in MANIFEST["entries"]]
        self.assertEqual(len(posters), len(set(posters)))
        self.assertTrue(all(path.startswith("thumbnails/showcase/") and path.endswith(".png") for path in posters))
        self.assertEqual(MANIFEST["publication"], "preview")


if __name__ == "__main__":
    unittest.main()
