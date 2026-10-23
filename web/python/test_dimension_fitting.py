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
        result._semantic_handle = types.SimpleNamespace(
            layoutAnchor=lambda index: self.anchor,
        )
        return result

    def setUp(self):
        self.calls = []
        self.anchor = types.SimpleNamespace(
            dimensionLength=lambda dim: 0.0 if dim == 2 else 1.0,
            stretch=lambda *args: self.calls.append(("stretch", args)),
            rescaleToFit=lambda *args: self.calls.append(("fit", args)),
            matchDimSize=lambda *args: self.calls.append(("match", args)),
        )

    def test_public_methods_pass_dimensions_without_reading_python_bounds(self):
        source, target = self.wrapper(), self.wrapper()
        self.assertIs(source.scale_to_fit_width(4), source)
        self.assertIs(source.match_height(target, stretch=True), source)
        self.assertEqual(self.calls, [("fit", (4.0, 0, False, 0., 0., 0., False)),
                                     ("match", (self.anchor, 1, True, 0., 0., 0., False))])

    def test_stretch_routes_only_dimension_and_pivot_arguments(self):
        source = self.wrapper()
        self.assertIs(source.stretch(-2, 0, about_edge=noon.LEFT), source)
        self.assertIs(source.stretch_about_point(3, 1, (2, 4)), source)
        self.assertEqual(self.calls, [("stretch", (-2., 0, -1., 0., 0., False)),
                                     ("stretch", (3., 1, 2., 4., 0., True))])

    def test_live_dispatch_passes_both_opaque_anchors(self):
        source, target = self.wrapper(), self.wrapper()
        context = types.SimpleNamespace(
            queryLayoutDimensionLength=lambda *args: 1.0,
            liveRescaleToFit=lambda *args: self.calls.append(("live fit", args)),
            liveMatchDimSize=lambda *args: self.calls.append(("live match", args)),
        )
        with patch.object(semantic, "_live_mutation_context", return_value=context):
            source.stretch_to_fit_height(3)
            source.match_width(target)
        self.assertEqual(self.calls, [("live fit", (self.anchor, 3.0, 1, True, 0., 0., 0., False)),
                                     ("live match", (self.anchor, self.anchor, 0, False, 0., 0., 0., False))])

    def test_explicit_pivots_are_forwarded_without_frontend_bounds(self):
        source, target = self.wrapper(), self.wrapper()
        source.scale_to_fit_width(4, about_edge=noon.LEFT)
        source.match_height(target, about_point=(2, 3))
        self.assertEqual(self.calls, [("fit", (4., 0, False, -1., 0., 0., False)),
                                     ("match", (self.anchor, 1, False, 2., 3., 0., True))])

    def test_depth_stretch_forwards_full_pivots_and_dimension(self):
        source = self.wrapper()
        source.stretch(-2, 2, about_point=(1, 2, 3.125))
        source.stretch(0, 2, about_edge=(0, 0, -1))
        context = types.SimpleNamespace(
            liveStretchLayout=lambda *args: self.calls.append(("live stretch", args)),
        )
        with patch.object(semantic, "_live_mutation_context", return_value=context):
            self.assertIs(source.stretch(2, 2), source)
        self.assertEqual(self.calls, [
            ("stretch", (-2., 2, 1., 2., 3.125, True)),
            ("stretch", (0., 2, 0., 0., -1., False)),
            ("live stretch", (self.anchor, 2., 2, 0., 0., 0., False)),
        ])

    def test_z_shift_is_forwarded_to_authored_and_live_rust(self):
        source = self.wrapper()
        source._semantic_handle.shift = lambda *args: self.calls.append(("shift", args))
        self.assertIs(source.shift((1, -2, 3.125)), source)
        context = types.SimpleNamespace(
            liveShift=lambda *args: self.calls.append(("live shift", args)),
        )
        with patch.object(semantic, "_live_mutation_context", return_value=context):
            source.shift((4, 5, -6.25))
        self.assertEqual(self.calls, [
            ("shift", (1., -2., 3.125)),
            ("live shift", (source._semantic_handle, 4., 5., -6.25)),
        ])

    def test_unpaired_anchor_error_preserves_the_typed_engine_cause(self):
        from _noon_errors import NoonErrorCause, NoonValueError
        source = self.wrapper()
        inner = NoonErrorCause("invalid_input", "path.unpaired_anchors", "native details")
        for diagnostic in (inner, NoonErrorCause("invalid_input", "live.authoring", "outer", inner)):
            error = NoonValueError(diagnostic, object(), None)
            def fail(*args):
                raise error
            self.anchor.rescaleToFit = fail
            with self.assertRaises(ValueError) as raised:
                source.scale_to_fit_width(2.)
            self.assertIs(type(raised.exception), ValueError)
            self.assertEqual(str(raised.exception), "zip() argument 2 is shorter than argument 1")
            self.assertIs(raised.exception.__cause__, error)

    def test_unsupported_arguments_fail_before_calling_shared_mutation(self):
        source = self.wrapper()
        with self.assertRaises(NotImplementedError):
            source.stretch(2, 3)
        for dimension in (-1, 3):
            with self.assertRaises(AssertionError):
                source.rescale_to_fit(2, dimension)
        with self.assertRaises(IndexError):
            source.rescale_to_fit(2, 1.0)
        with self.assertRaises(TypeError):
            source.scale_to_fit_width(2, unknown_pivot=noon.ORIGIN)
        with self.assertRaises(TypeError):
            source.match_width(object())
        self.assertEqual(self.calls, [])

    def test_depth_and_zero_extent_skip_length_and_pivot_coercion(self):
        source, target = self.wrapper(), self.wrapper()
        self.assertIs(source.rescale_to_fit(object(), 2, about_point=object()), source)
        self.assertIs(source.match_dim_size(target, 2, about_edge=object()), source)
        self.anchor.dimensionLength = lambda dim: 0.0
        self.assertIs(source.rescale_to_fit(object(), 0, about_point=object()), source)
        self.assertEqual(self.calls, [
            ("fit", (0.0, 2, False, 0., 0., 0., False)),
            ("match", (self.anchor, 2, False, 0., 0., 0., False)),
            ("fit", (0.0, 0, False, 0., 0., 0., False)),
        ])
