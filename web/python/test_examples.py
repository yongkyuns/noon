import json
import runpy
import unittest
from pathlib import Path

from noon import PatchBatch, Scene


EXAMPLES_DIR = Path(__file__).with_name("examples")
SCENE_EXAMPLES = (
    "lifecycle_handoffs.py",
    "fade_appearance.py",
    "matching_shapes.py",
    "staggered_choreography.py",
    "instanced_field.py",
    "path_reveal.py",
    "filled_path_transform.py",
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

    def test_gallery_uses_public_semantic_vocabulary(self) -> None:
        paths = [Path(__file__).with_name("demo_scene.py")]
        paths.extend(EXAMPLES_DIR / filename for filename in SCENE_EXAMPLES)
        for path in paths:
            with self.subTest(path=path.name):
                source = path.read_text(encoding="utf-8")
                self.assertNotIn("from noon_layout", source)
                self.assertNotIn("Color(", source)

    def test_instanced_field_survives_row_and_column_source_edits(self) -> None:
        source_path = EXAMPLES_DIR / "instanced_field.py"
        original = source_path.read_text()
        for columns, rows in ((20, 10), (1, 1)):
            with self.subTest(columns=columns, rows=rows):
                source = original.replace("COLUMNS = 18", f"COLUMNS = {columns}", 1).replace(
                    "ROWS = 10", f"ROWS = {rows}", 1
                )
                namespace: dict[str, object] = {}
                exec(compile(source, str(source_path), "exec"), namespace)
                result = namespace.get("result")
                self.assertIsInstance(result, Scene)
                document = result.to_document()
                self.assertEqual(len(document["objects"]), columns * rows)
                self.assertEqual(len(document["tracks"]), columns * rows)
                self.assertEqual(
                    {track["property"] for track in document["tracks"]},
                    {"position"},
                )

    def test_path_reveal_example_is_only_about_reveal(self) -> None:
        namespace = runpy.run_path(EXAMPLES_DIR / "path_reveal.py")
        result = namespace.get("result")
        self.assertIsInstance(result, Scene)
        properties = [track["property"] for track in result.to_document()["tracks"]]
        self.assertEqual(properties, ["reveal"])

    def test_fade_example_preserves_semantic_opacity(self) -> None:
        namespace = runpy.run_path(EXAMPLES_DIR / "fade_appearance.py")
        result = namespace.get("result")
        self.assertIsInstance(result, Scene)
        document = result.to_document()
        self.assertEqual(document["objects"][1]["style"]["opacity"], 0.42)
        properties = [track["property"] for track in document["tracks"]]
        self.assertEqual(properties.count("appearance"), 2)
        self.assertEqual(properties.count("presence"), 2)

    def test_matching_shapes_example_lowers_to_three_handoffs(self) -> None:
        namespace = runpy.run_path(EXAMPLES_DIR / "matching_shapes.py")
        result = namespace.get("result")
        self.assertIsInstance(result, Scene)
        properties = [track["property"] for track in result.to_document()["tracks"]]
        self.assertEqual(properties.count("transform"), 3)
        self.assertEqual(properties.count("presence"), 6)

    def test_morph_stress_example_is_focused_on_morph_reuse(self) -> None:
        namespace = runpy.run_path(EXAMPLES_DIR / "morph_stress_test.py")
        result = namespace.get("result")
        self.assertIsInstance(result, Scene)
        document = result.to_document()

        self.assertEqual(len(document["objects"]), 600)
        properties = [track["property"] for track in document["tracks"]]
        self.assertEqual(properties, ["transform"] * 600)

        morph_geometries = {
            json.dumps(
                track["values"]["object"]["to"]["geometry"]["vector_path"],
                sort_keys=True,
                separators=(",", ":"),
            )
            for track in document["tracks"]
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
                self.assertEqual(properties, ["transform"] * object_count)

                morph_geometries = {
                    json.dumps(
                        track["values"]["object"]["to"]["geometry"]["vector_path"],
                        sort_keys=True,
                        separators=(",", ":"),
                    )
                    for track in document["tracks"]
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
