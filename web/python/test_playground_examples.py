import json
import unittest
from pathlib import Path

from noon import PatchBatch, Scene
from playground_examples import (
    PLAYGROUND_PATCH_EXAMPLES,
    PLAYGROUND_SCENE_EXAMPLES,
    run_patch_example,
    run_scene_example,
)


class PlaygroundExampleTests(unittest.TestCase):
    def test_every_registered_scene_executes(self) -> None:
        for name, relative_path, context in PLAYGROUND_SCENE_EXAMPLES:
            with self.subTest(name=name):
                self.assertIsInstance(run_scene_example(relative_path, context), Scene)

    def test_every_registered_scene_fits_the_four_second_loop(self) -> None:
        for name, relative_path, context in PLAYGROUND_SCENE_EXAMPLES:
            with self.subTest(name=name):
                document = run_scene_example(relative_path, context).to_document()
                latest_end = max(
                    track["timing"]["start_time"] + track["timing"]["duration"]
                    for track in document["tracks"]
                )
                self.assertLess(latest_end, 4.0)

    def test_every_registered_patch_executes(self) -> None:
        for name, relative_path, context in PLAYGROUND_PATCH_EXAMPLES:
            with self.subTest(name=name):
                self.assertIsInstance(run_patch_example(relative_path, context), PatchBatch)

    def test_scene_catalog_is_curated_without_duplicate_sources(self) -> None:
        names = [name for name, _, _ in PLAYGROUND_SCENE_EXAMPLES]
        paths = [path for _, path, _ in PLAYGROUND_SCENE_EXAMPLES]
        self.assertGreater(len(paths), 0)
        self.assertEqual(len(names), len(set(names)))
        self.assertEqual(len(paths), len(set(paths)))
        self.assertTrue(all(path.startswith("python/") for path in paths))

    def test_public_gallery_is_manifest_driven_and_manim_only(self) -> None:
        web_root = Path(__file__).parents[1]
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
                self.assertIn(entry["parity_status"], {"candidate", "parity-qualified"})
                self.assertTrue(entry["parity_fixture"])
                self.assertTrue((web_root / entry["path"]).is_file())
                self.assertTrue((web_root / entry["thumbnail"]).is_file())

        # Internal Noon-native examples remain useful for renderer/runtime regression
        # tests, but they are intentionally disjoint from the public Manim gallery.
        internal_scene_paths = {
            relative_path for _, relative_path, _ in PLAYGROUND_SCENE_EXAMPLES
        }
        public_scene_paths = {entry["path"] for entry in ready}
        self.assertTrue(internal_scene_paths.isdisjoint(public_scene_paths))


if __name__ == "__main__":
    unittest.main()
