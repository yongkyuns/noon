import json
import re
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

    def test_python_catalog_matches_javascript_picker(self) -> None:
        web_root = Path(__file__).parents[1]
        main_js = (web_root / "main.js").read_text(encoding="utf-8")
        scene_block = main_js.split("const SCENE_EXAMPLES = [", 1)[1].split("\n];", 1)[0]
        patch_block = main_js.split("const PATCH_EXAMPLES = [", 1)[1].split("\n];", 1)[0]

        registered_scene_paths = re.findall(r'path:\s*"(\./python/[^\"]+\.py)"', scene_block)
        registered_patch_paths = re.findall(r'path:\s*"(\./python/[^\"]+\.py)"', patch_block)

        native_scene_paths = [
            f"./{relative_path}" for _, relative_path, _ in PLAYGROUND_SCENE_EXAMPLES
        ]
        manifest = json.loads(
            (web_root / "python" / "examples" / "manim_tutorial_manifest.json").read_text(
                encoding="utf-8"
            )
        )
        manim_scene_paths = [
            f"./{entry['path']}"
            for entry in manifest["entries"]
            if entry["status"] == "ready"
        ]
        self.assertTrue(manim_scene_paths)
        self.assertTrue(set(native_scene_paths).isdisjoint(manim_scene_paths))

        # The gallery starts with Noon's basic scene, then teaches the browser-only
        # Manim compatibility surface before continuing into renderer/perf examples.
        expected_scene_paths = [
            native_scene_paths[0],
            *manim_scene_paths,
            *native_scene_paths[1:],
        ]
        expected_patch_paths = [
            f"./{relative_path}" for _, relative_path, _ in PLAYGROUND_PATCH_EXAMPLES
        ]

        self.assertEqual(registered_scene_paths, expected_scene_paths)
        self.assertEqual(registered_patch_paths, expected_patch_paths)


if __name__ == "__main__":
    unittest.main()
