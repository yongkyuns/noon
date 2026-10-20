"""Circle sizing is one shared Rust operation; Python owns call normalization."""
import types
import hashlib
import json
from pathlib import Path
import unittest
from unittest.mock import Mock, patch

import noon
import _manim_semantic_handles as semantic
from _typed_geometry_test_support import identity_only_wrapper


class CircleSurroundTests(unittest.TestCase):
    def test_original_reference_body_matches_pinned_directive_inventory(self):
        root = Path(__file__).resolve().parents[2]
        source = (root / "parity/manim-v0.21/upstream-examples/circle_surround.py").read_text()
        body = source[source.index("class CircleSurround(Scene):"):].rstrip()
        inventory = json.loads((root / "compat/manim-v0.21.0-reference-dispositions.json").read_text())
        reference = next(row["reference_provenance"] for row in inventory["entries"]
                         if row["reference_provenance"]["name"] == "CircleSurround")
        self.assertEqual(hashlib.sha256(body.encode()).hexdigest(), reference["source_sha256"])
        manifest = json.loads((root / "parity/manim-v0.21/manifest.json").read_text())
        fixture = next(row for row in manifest["fixtures"] if row["id"] == "circle-surround-upstream")
        self.assertEqual(fixture["source"], "parity/manim-v0.21/upstream-examples/circle_surround.py")
        self.assertEqual(fixture["scene"], "CircleSurround")
        self.assertEqual(fixture["expected_duration"], 0.0)
        self.assertEqual(fixture["expected_object_count"], 6)
        self.assertNotIn("raster_tolerance", fixture)

    def setUp(self):
        self.circle = identity_only_wrapper(noon.Circle)
        self.target = identity_only_wrapper(noon.VGroup)
        self.source_anchor = types.SimpleNamespace(surroundCircle=Mock())
        self.target_anchor = object()

    def test_nonnegative_dimensions_delegate_without_geometry_queries(self):
        with patch.object(semantic, "_replacement_anchors", return_value=(self.source_anchor, self.target_anchor, None)):
            self.assertIs(self.circle.surround(self.target), self.circle)
            self.assertIs(self.circle.surround(self.target, 2, buffer_factor=-0.5), self.circle)
        self.assertEqual(self.source_anchor.surroundCircle.call_args_list, [
            unittest.mock.call(self.target_anchor, 0, False, 1.2),
            unittest.mock.call(self.target_anchor, 2, False, -0.5),
        ])

    def test_stretch_ignores_dimension_even_when_it_is_not_an_index(self):
        with patch.object(semantic, "_replacement_anchors", return_value=(self.source_anchor, self.target_anchor, None)):
            self.circle.surround(self.target, object(), stretch=True)
        self.source_anchor.surroundCircle.assert_called_once_with(self.target_anchor, 0, True, 1.2)

    def test_live_operation_uses_the_selected_owner_once(self):
        context = types.SimpleNamespace(liveSurroundCircle=Mock())
        with patch.object(semantic, "_replacement_anchors", return_value=(self.source_anchor, self.target_anchor, context)):
            self.assertIs(self.circle.surround(self.target, 1, True, 0.8), self.circle)
        context.liveSurroundCircle.assert_called_once_with(self.source_anchor, self.target_anchor, 0, True, 0.8)
        self.source_anchor.surroundCircle.assert_not_called()

    def test_generic_buff_keyword_and_invalid_dimensions_reject_before_rust(self):
        with patch.object(semantic, "_replacement_anchors") as prepare:
            for dimension in [-4, -3, -2, -1, -0.5, 3, float("nan"), float("inf")]:
                with self.assertRaises(AssertionError):
                    self.circle.surround(self.target, dimension)
            with self.assertRaises(IndexError):
                self.circle.surround(self.target, 1.0)
            with self.assertRaises(TypeError):
                self.circle.surround(self.target, object())
            with self.assertRaises(TypeError):
                self.circle.surround(self.target, buff=0.2)
            with self.assertRaises(TypeError):
                self.circle.surround(object())
            prepare.assert_not_called()

    def test_staged_callback_rejects_before_layout_reads(self):
        with patch("_manim_updaters._canonical_phase_context", return_value=object()), \
             patch.object(semantic, "_replacement_anchors") as prepare:
            with self.assertRaisesRegex(NotImplementedError, "non-staged layout"):
                self.circle.surround(self.target)
            prepare.assert_not_called()
