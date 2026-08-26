import ast
import json
import unittest
from pathlib import Path

WEB_ROOT = Path(__file__).parents[1]
REPO_ROOT = WEB_ROOT.parent
MANIFEST_PATH = WEB_ROOT / "python" / "examples" / "manim_tutorial_manifest.json"
PARITY_MANIFEST_PATH = REPO_ROOT / "parity" / "manim-v0.21" / "manifest.json"


def load_manifest():
    return json.loads(MANIFEST_PATH.read_text(encoding="utf-8"))


def load_parity_manifest():
    return json.loads(PARITY_MANIFEST_PATH.read_text(encoding="utf-8"))


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

    def test_source_equivalent_demo_entries_link_to_canonical_parity_fixtures(self) -> None:
        entries = load_manifest()["entries"]
        parity_entries = [
            entry
            for entry in entries
            if entry.get("parity_status") in {"candidate", "parity-qualified"}
        ]
        self.assertGreaterEqual(len(parity_entries), 5)

        canonical_fixtures = {
            fixture["id"] for fixture in load_parity_manifest()["fixtures"]
        }
        linked_fixtures = []
        for entry in parity_entries:
            with self.subTest(entry=entry["id"]):
                self.assertEqual(entry["reuse"], "source-equivalent-manim-v0.21")
                fixture = entry["parity_fixture"]
                linked_fixtures.append(fixture)
                self.assertIn(fixture, canonical_fixtures)
                self.assertIn("pixel-parity", entry["features"])
                self.assertIn("time-parity", entry["features"])

        self.assertEqual(len(linked_fixtures), len(set(linked_fixtures)))

    def test_adapted_ready_examples_are_not_mislabeled_as_parity(self) -> None:
        for entry in load_manifest()["entries"]:
            if entry.get("reuse") == "original-noon-adaptation":
                with self.subTest(entry=entry["id"]):
                    self.assertEqual(entry.get("parity_status"), "adaptation")


if __name__ == "__main__":
    unittest.main()
