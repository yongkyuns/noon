import unittest
from pathlib import Path

import _manim_animate as animate


class TransformMatchingShapesAdapterTests(unittest.TestCase):
    def test_constructor_is_inert_and_matches_supported_manim_call_shape(self) -> None:
        source = object()
        target = object()
        request = animate.TransformMatchingShapes(source, target, run_time=2.0)
        self.assertIs(request.source, source)
        self.assertIs(request.target, target)
        self.assertIs(request.mobject, source)
        self.assertIs(request.target_mobject, target)
        self.assertEqual(request.anim_args, {"run_time": 2.0})
        self.assertEqual(request.key_map, {})

    def test_unsupported_mismatch_modes_fail_closed(self) -> None:
        source = object()
        target = object()
        with self.assertRaises(NotImplementedError):
            animate.TransformMatchingShapes(source, target, transform_mismatches=True)
        with self.assertRaises(NotImplementedError):
            animate.TransformMatchingShapes(source, target, fade_transform_mismatches=True)
        with self.assertRaises(NotImplementedError):
            animate.TransformMatchingShapes(source, target, key_map={"a": "b"})
        with self.assertRaises(TypeError):
            animate.TransformMatchingShapes(source, target, key_map=[("a", "b")])
        animate.TransformMatchingShapes(source, target, key_map={})

    def test_python_only_selects_the_shared_matching_family_request(self) -> None:
        python_dir = Path(__file__).resolve().parent
        scene_source = (python_dir / "_manim_scene.py").read_text()
        rust_source = (
            python_dir.parent.parent
            / "crates"
            / "noon-web"
            / "src"
            / "canonical_authoring_scene.rs"
        ).read_text()
        self.assertIn("appendMatchingFamilyTransformTo", scene_source)
        self.assertIn("MatchingFamilyTransformTo", rust_source)
        self.assertNotIn("matching_shape_key", scene_source)
        self.assertNotIn("matching_shape_correspondence", scene_source)


if __name__ == "__main__":
    unittest.main()
