import json
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
    "path_reveal.py",
    "path_morph_transform.py",
    "morph_stress_test.py",
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

    def test_instanced_field_survives_row_and_column_source_edits(self) -> None:
        source_path = EXAMPLES_DIR / "instanced_field.py"
        original = source_path.read_text()
        for columns, rows in ((20, 10), (1, 1)):
            with self.subTest(columns=columns, rows=rows):
                source = original.replace("columns = 18", f"columns = {columns}", 1).replace(
                    "rows = 10", f"rows = {rows}", 1
                )
                namespace: dict[str, object] = {}
                exec(compile(source, str(source_path), "exec"), namespace)
                result = namespace.get("result")
                self.assertIsInstance(result, Scene)
                document = result.to_document()
                self.assertEqual(len(document["objects"]), columns * rows)
                self.assertEqual(len(document["tracks"]), columns * rows * 2)

    def test_path_reveal_example_contains_reveal_tracks(self) -> None:
        namespace = runpy.run_path(EXAMPLES_DIR / "path_reveal.py")
        result = namespace.get("result")
        self.assertIsInstance(result, Scene)
        properties = [track["property"] for track in result.to_document()["tracks"]]
        self.assertGreaterEqual(properties.count("reveal"), 2)

    def test_morph_stress_example_exercises_reuse_and_many_active_tracks(self) -> None:
        namespace = runpy.run_path(EXAMPLES_DIR / "morph_stress_test.py")
        result = namespace.get("result")
        self.assertIsInstance(result, Scene)
        document = result.to_document()

        self.assertEqual(len(document["objects"]), 600)
        properties = [track["property"] for track in document["tracks"]]
        self.assertEqual(properties.count("transform"), 600)
        self.assertEqual(properties.count("rotation"), 600)

        morph_geometries = {
            json.dumps(
                track["values"]["object"]["to"]["geometry"]["vector_path"],
                sort_keys=True,
                separators=(",", ":"),
            )
            for track in document["tracks"]
            if track["property"] == "transform"
        }
        self.assertEqual(len(morph_geometries), 12)

    def test_morph_stress_scale_presets_preserve_twelve_geometry_variants(self) -> None:
        for object_count in (1_000, 3_000):
            with self.subTest(object_count=object_count):
                namespace = runpy.run_path(
                    EXAMPLES_DIR / "morph_stress_test.py",
                    init_globals={"context": {"object_count": object_count}},
                )
                result = namespace.get("result")
                self.assertIsInstance(result, Scene)
                document = result.to_document()
                self.assertEqual(len(document["objects"]), object_count)

                properties = [track["property"] for track in document["tracks"]]
                self.assertEqual(properties.count("transform"), object_count)
                self.assertEqual(properties.count("rotation"), object_count)

                morph_geometries = {
                    json.dumps(
                        track["values"]["object"]["to"]["geometry"]["vector_path"],
                        sort_keys=True,
                        separators=(",", ":"),
                    )
                    for track in document["tracks"]
                    if track["property"] == "transform"
                }
                self.assertEqual(len(morph_geometries), 12)

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
