"""Scale argument adaptation delegates complete coordinates and identity to Rust."""
import types
import unittest
from unittest.mock import patch

try:
    import numpy as np
except ImportError:
    np = None
import noon
import _manim_semantic_handles as semantic
from _typed_geometry_test_support import identity_only_wrapper


class ScaleXYZTests(unittest.TestCase):
    def test_scalar_vector_and_pivots_reach_the_same_authored_live_operation(self):
        for family in (False, True):
            for live in (False, True):
                with self.subTest(family=family, live=live):
                    calls = []
                    anchor = types.SimpleNamespace(scale3d=lambda *args: calls.append(args))
                    handle = types.SimpleNamespace(layoutAnchor=lambda index: anchor, memberKeys=lambda: [])
                    source = identity_only_wrapper(noon.VGroup if family else noon.Mobject, submobjects=[])
                    source._semantic_handle = handle
                    source._semantic_handle_fresh = True
                    if family:
                        source._semantic_family_handle = handle
                    context = types.SimpleNamespace(liveScaleLayout3d=lambda selected, *args: (
                        self.assertIs(selected, anchor), calls.append(args))) if live else None
                    resolver = "_group_live_layout_context" if family else "_live_mutation_context"
                    with patch.object(semantic, resolver, return_value=context):
                        self.assertIs(source.scale(2), source)
                        source.scale([-.7, 1.3, 2.5], about_point=(1, -2, 4.25))
                        source.scale(-1.25, about_edge=(1, -1, 1))
                    self.assertEqual(calls, [(2., 2., 2., 0., 0., 0., False),
                                             (-.7, 1.3, 2.5, 1., -2., 4.25, True),
                                             (-1.25, -1.25, -1.25, 1., -1., 1., False)])

    def test_numeric_array_broadcasts_and_planar_convenience(self):
        for value, expected in (([2], (2., 2., 2.)), ((2, 3), (2., 3., 1.)),
                                (noon.Vec2(2, 3), (2., 3., 1.))):
            self.assertEqual(semantic._scale_factors_xyz(value), expected)

    @unittest.skipIf(np is None, "NumPy argument adaptation requires NumPy")
    def test_numpy_numeric_array_factors(self):
        self.assertEqual(semantic._scale_factors_xyz(np.array(2.)), (2., 2., 2.))
        self.assertEqual(semantic._scale_factors_xyz(np.array([-.7, 1.3, 2.5])),
                         (-.7, 1.3, 2.5))

    def test_unpaired_anchor_error_maps_to_ce_value_error(self):
        native = types.SimpleNamespace(code="path.unpaired_anchors")
        error = RuntimeError("typed authoring failure")
        error.rust_cause = native
        with self.assertRaisesRegex(ValueError, "zip\\(\\) argument 2 is shorter than argument 1") as caught:
            semantic._raise_boundary_error(error)
        self.assertIs(caught.exception.__cause__, error)
