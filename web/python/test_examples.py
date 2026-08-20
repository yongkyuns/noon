import runpy
import unittest
from pathlib import Path

from noon import PatchBatch, Scene


EXAMPLES_DIR = Path(__file__).with_name("examples")
SCENE_EXAMPLES = (
    "staggered_choreography.py",
    "vector_path_garden.py",
    "instanced_field.py",
    "kinetic_lines.py",
    "mixed_geometry.py",
)


class PlaygroundExampleTests(unittest.TestCase):
    def test_scene_examples_build_valid_scene_documents(self) -> None:
        for filename in SCENE_EXAMPLES:
            with self.subTest(filename=filename):
                namespace = runpy.run_path(EXAMPLES_DIR / filename)
                result = namespace.get("result")
                self.assertIsInstance(result, Scene)
                document = result.to_document()
                self.assertEqual(document["version"], 1)
                self.assertGreater(len(document["objects"]), 0)
                self.assertGreater(len(document["tracks"]), 0)

    def test_transform_patch_builds_ordered_patch_batch(self) -> None:
        namespace = runpy.run_path(
            EXAMPLES_DIR / "transform_patch.py",
            init_globals={"context": {"sequence": 8}},
        )
        result = namespace.get("result")
        self.assertIsInstance(result, PatchBatch)
        document = result.to_document()
        self.assertEqual(document["version"], 1)
        self.assertEqual(document["sequence"], 8)
        self.assertEqual(len(document["patches"]), 3)


if __name__ == "__main__":
    unittest.main()
