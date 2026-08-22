import runpy
import unittest
from pathlib import Path

from noon import Scene


class FeatureShowcaseTests(unittest.TestCase):
    def _run_showcase(self) -> Scene:
        example = Path(__file__).parent / "examples" / "feature_showcase.py"
        namespace = runpy.run_path(example)
        result = namespace["result"]
        self.assertIsInstance(result, Scene)
        return result

    def test_showcase_exercises_new_animation_features(self) -> None:
        document = self._run_showcase().to_document()
        properties = [track["property"] for track in document["tracks"]]

        self.assertGreaterEqual(properties.count("transform"), 6)
        self.assertGreaterEqual(properties.count("presence"), 10)
        self.assertEqual(properties.count("appearance"), 2)

        # first, middle, last, copy-source, copy-target
        self.assertEqual(document["objects"][4]["style"]["opacity"], 0.42)

    def test_showcase_is_registered_in_the_playground_gallery(self) -> None:
        main_js = (Path(__file__).parents[1] / "main.js").read_text()
        self.assertIn("./python/examples/feature_showcase.py", main_js)
        self.assertIn("Lifecycle · matching · Fade", main_js)


if __name__ == "__main__":
    unittest.main()
