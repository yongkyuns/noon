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

    def test_every_registered_patch_executes(self) -> None:
        for name, relative_path, context in PLAYGROUND_PATCH_EXAMPLES:
            with self.subTest(name=name):
                self.assertIsInstance(run_patch_example(relative_path, context), PatchBatch)

    def test_scene_catalog_is_curated_without_duplicate_sources(self) -> None:
        names = [name for name, _, _ in PLAYGROUND_SCENE_EXAMPLES]
        paths = [path for _, path, _ in PLAYGROUND_SCENE_EXAMPLES]
        self.assertEqual(len(names), len(set(names)))
        self.assertEqual(len(paths), len(set(paths)))
        self.assertLessEqual(len(paths), 10)

    def test_python_catalog_matches_javascript_picker(self) -> None:
        main_js = (Path(__file__).parents[1] / "main.js").read_text(encoding="utf-8")
        scene_block = main_js.split("const SCENE_EXAMPLES = [", 1)[1].split("\n];", 1)[0]
        patch_block = main_js.split("const PATCH_EXAMPLES = [", 1)[1].split("\n];", 1)[0]

        registered_scene_paths = re.findall(r'path:\s*"(\./python/[^\"]+\.py)"', scene_block)
        registered_patch_paths = re.findall(r'path:\s*"(\./python/[^\"]+\.py)"', patch_block)
        expected_scene_paths = [f"./{relative_path}" for _, relative_path, _ in PLAYGROUND_SCENE_EXAMPLES]
        expected_patch_paths = [f"./{relative_path}" for _, relative_path, _ in PLAYGROUND_PATCH_EXAMPLES]

        self.assertEqual(registered_scene_paths, expected_scene_paths)
        self.assertEqual(registered_patch_paths, expected_patch_paths)


if __name__ == "__main__":
    unittest.main()
