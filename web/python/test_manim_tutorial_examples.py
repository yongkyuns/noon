import json
import unittest
from pathlib import Path

from noon import Scene
from playground_examples import run_scene_example

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

    def test_every_ready_tutorial_executes(self) -> None:
        ready = [entry for entry in load_manifest()["entries"] if entry["status"] == "ready"]
        self.assertGreaterEqual(len(ready), 7)
        paths = []
        for entry in ready:
            with self.subTest(entry=entry["id"]):
                path = entry["path"]
                paths.append(path)
                self.assertTrue((WEB_ROOT / path).is_file())
                self.assertIsInstance(run_scene_example(path, {}), Scene)
        self.assertEqual(len(paths), len(set(paths)))

    def test_ready_tutorials_fit_interactive_demo_loop(self) -> None:
        ready = [entry for entry in load_manifest()["entries"] if entry["status"] == "ready"]
        for entry in ready:
            with self.subTest(entry=entry["id"]):
                document = run_scene_example(entry["path"], {}).to_document()
                ends = [
                    track["timing"]["start_time"] + track["timing"]["duration"]
                    for track in document.get("tracks", [])
                ]
                ends.extend(
                    track["timing"]["start_time"] + track["timing"]["duration"]
                    for track in document.get("signal_tracks", [])
                )
                self.assertTrue(ends, f"{entry['id']} should exercise timed behavior")
                self.assertLess(max(ends), 4.0)

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
