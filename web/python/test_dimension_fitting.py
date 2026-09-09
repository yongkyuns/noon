"""The facade passes identity and intent; layout arithmetic stays in Rust."""
import types
import unittest
from unittest.mock import patch

import noon
import _manim_semantic_handles as semantic


class DimensionFittingTests(unittest.TestCase):
    def wrapper(self):
        result = object.__new__(noon.Mobject)
        result._scene = None
        result._object = None
        result._semantic_handle_fresh = True
        result._semantic_handle = types.SimpleNamespace(layoutAnchor=lambda index: self.anchor)
        return result

    def setUp(self):
        self.calls = []
        self.anchor = types.SimpleNamespace(
            rescaleToFit=lambda *args: self.calls.append(("fit", args)),
            matchDimSize=lambda *args: self.calls.append(("match", args)),
        )

    def test_public_methods_pass_dimensions_without_reading_python_bounds(self):
        source, target = self.wrapper(), self.wrapper()
        self.assertIs(source.scale_to_fit_width(4), source)
        self.assertIs(source.match_height(target, stretch=True), source)
        self.assertEqual(self.calls, [("fit", (4.0, 0, False)),
                                     ("match", (self.anchor, 1, True))])

    def test_live_dispatch_passes_both_opaque_anchors(self):
        source, target = self.wrapper(), self.wrapper()
        context = types.SimpleNamespace(
            liveRescaleToFit=lambda *args: self.calls.append(("live fit", args)),
            liveMatchDimSize=lambda *args: self.calls.append(("live match", args)),
        )
        with patch.object(semantic, "_live_mutation_context", return_value=context):
            source.stretch_to_fit_height(3)
            source.match_width(target)
        self.assertEqual(self.calls, [("live fit", (self.anchor, 3.0, 1, True)),
                                     ("live match", (self.anchor, self.anchor, 0, False))])

    def test_unsupported_arguments_fail_before_calling_shared_mutation(self):
        source = self.wrapper()
        with self.assertRaises(NotImplementedError):
            source.rescale_to_fit(2, 2)
        with self.assertRaises(NotImplementedError):
            source.scale_to_fit_width(2, about_point=noon.ORIGIN)
        with self.assertRaises(TypeError):
            source.match_width(object())
        self.assertEqual(self.calls, [])
