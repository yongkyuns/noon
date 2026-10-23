"""Depth observations and fitting route through opaque shared Rust anchors."""
import types
import unittest
from unittest.mock import patch
import noon
import _manim_semantic_handles as semantic


class ManimMobjectDepthTests(unittest.TestCase):
    def setUp(self):
        self.calls = []
        self.anchor = types.SimpleNamespace(
            dimensionLength=lambda dim: 0.0,
            rescaleToFit=lambda *args: self.calls.append(("fit", args)),
            matchDimSize=lambda *args: self.calls.append(("match", args)),
        )

    def wrapper(self, cls=noon.Mobject):
        value = object.__new__(cls)
        value._scene = None
        value._object = None
        value._semantic_handle_fresh = True
        value._semantic_handle = types.SimpleNamespace(layoutAnchor=lambda index: self.anchor)
        return value

    def test_planar_depth_fitting_preserves_receiver_without_coercing_pivots(self):
        value = self.wrapper()
        self.assertEqual(value.depth, 0.0)
        self.assertIs(value.scale_to_fit_depth(object(), about_point=object()), value)
        self.assertIs(value.stretch_to_fit_depth(object(), about_edge=object()), value)
        value.depth = object()
        self.assertEqual(self.calls, [("fit", (0., 2, False, 0., 0., 0., False)),
                                     ("fit", (0., 2, True, 0., 0., 0., False)),
                                     ("fit", (0., 2, False, 0., 0., 0., False))])

    def test_group_depth_reads_the_shared_anchor(self):
        group = self.wrapper(noon.VGroup)
        group._semantic_family_handle = object()
        self.anchor.dimensionLength = lambda dim: 6.25
        with patch.object(semantic, "_layout_anchor", return_value=self.anchor), \
             patch.object(semantic, "_group_live_layout_context", return_value=None):
            self.assertEqual(group.depth, 6.25)
            self.assertIs(group.scale_to_fit_depth(7.), group)
        self.assertEqual(self.calls, [("fit", (7., 2, False, 0., 0., 0., False))])

    def test_matching_depth_forwards_both_opaque_anchors(self):
        value, target = self.wrapper(), self.wrapper()
        self.assertIs(value.match_depth(target, about_point=object()), value)
        self.assertEqual(self.calls, [("match", (self.anchor, 2, False, 0., 0., 0., False))])

    def test_nonzero_leaf_depth_comes_from_rust(self):
        value = self.wrapper()
        self.anchor.dimensionLength = lambda dim: 70.75
        self.assertEqual(value.depth, 70.75)


if __name__ == "__main__":
    unittest.main()
