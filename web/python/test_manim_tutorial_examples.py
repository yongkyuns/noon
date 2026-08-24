import ast
import json
import unittest
from pathlib import Path

WEB_ROOT = Path(__file__).parents[1]
MANIFEST_PATH = WEB_ROOT / "python" / "examples" / "manim_tutorial_manifest.json"


def load_manifest():
    return json.loads(MANIFEST_PATH.read_text(encoding="utf-8"))


class ManimTutorialExampleTests(unittest.TestCase):
    def test_manifest_is_versioned_and_has_unique_ids(self) -> None:
        manifest = load_manifest()
        self.assertEqual(manifest["reference"]["version"], "0.21.0")
        entries = manifest["entries"]
        ids = [entry["id"] for entry in entries]
        self.assertEqual(len(ids), len(set(ids)))
        self.assertGreaterEqual(len(entries), 10)

    def test_every_ready_tutorial_exists_and_compiles(self) -> None:
        ready = [entry for entry in load_manifest()["entries"] if entry["status"] == "ready"]
        self.assertGreaterEqual(len(ready), 7)
        paths = []
        for entry in ready:
            with self.subTest(entry=entry["id"]):
                relative_path = entry["path"]
                paths.append(relative_path)
                path = WEB_ROOT / relative_path
                self.assertTrue(path.is_file())
                source = path.read_text(encoding="utf-8")
                tree = ast.parse(source, filename=str(path))
                self.assertTrue(
                    any(
                        isinstance(node, ast.Assign)
                        and any(isinstance(target, ast.Name) and target.id == "result" for target in node.targets)
                        for node in ast.walk(tree)
                    ),
                    f"{entry['id']} must assign result",
                )
                compile(tree, str(path), "exec")
        self.assertEqual(len(paths), len(set(paths)))

    def test_unready_tutorials_explain_their_dependency(self) -> None:
        entries = load_manifest()["entries"]
        for entry in entries:
            if entry["status"] in {"blocked", "deferred"}:
                with self.subTest(entry=entry["id"]):
                    self.assertIn("dependency", entry)
                    self.assertTrue(entry["dependency"].startswith("#"))

    def test_reuse_provenance_is_explicit(self) -> None:
        for entry in load_manifest()["entries"]:
            if entry["status"] == "ready":
                with self.subTest(entry=entry["id"]):
                    self.assertIn("upstream", entry)
                    self.assertIn("reuse", entry)


if __name__ == "__main__":
    unittest.main()
