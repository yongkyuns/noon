import sys
import types
import unittest
from unittest.mock import Mock, patch

import _manim_compat as compat
import _manim_path_editing as editing
import _manim_path_queries as queries
from _noon_errors import NoonErrorCause, NoonValueError
from _typed_geometry_test_support import identity_only_wrapper


class EmptyPointArrayTests(unittest.TestCase):
    def assert_empty_dispatch(self, points, shape):
        ffi = types.ModuleType("pyodide.ffi")
        ffi.to_js = lambda values: values
        for cls in (compat.Mobject, compat.VMobject, compat.Group, compat.VGroup):
            for live in (False, True):
                with self.subTest(cls=cls.__name__, live=live, shape=shape):
                    value = identity_only_wrapper(cls)
                    handle, context = Mock(), Mock() if live else None
                    family = cls in (compat.Group, compat.VGroup)
                    if family:
                        value._semantic_family_handle = handle
                    with patch.dict(sys.modules, {"pyodide.ffi": ffi}), \
                         patch.object(editing, "_handle_for", return_value=handle), \
                         patch.object(editing, "_live_mutation_context", return_value=context), \
                         patch.object(editing, "_live_constructor_context", return_value=None):
                        self.assertIs(editing.set_points(value, points), value)
                    if live:
                        name = "liveSetFamilyEmptyPointArray" if family else "liveSetEmptyPointArray"
                        getattr(context, name).assert_called_once_with(handle, list(shape))
                        handle.setEmptyPointArray.assert_not_called()
                    else:
                        handle.setEmptyPointArray.assert_called_once_with(list(shape))
                    handle.setPoints3D.assert_not_called()

    def test_concrete_empty_lists_and_tuples_forward_rank_one(self):
        for points in ([], ()):
            self.assert_empty_dispatch(points, (0,))

    def test_numpy_empty_arrays_forward_every_retained_dimension(self):
        try:
            import numpy as np
        except ImportError:
            self.skipTest("NumPy input shapes")
        for shape in ((0,), (0, 2), (0, 3), (0, 2, 3)):
            self.assert_empty_dispatch(np.empty(shape), shape)

    def test_shape_inspection_does_not_invoke_custom_sequence_hooks(self):
        calls = []
        class Sequence:
            @property
            def shape(self):
                calls.append("shape")
                raise AssertionError("extra shape access")
            def __len__(self):
                calls.append("len")
                raise AssertionError("extra length access")
            def __iter__(self):
                calls.append("iter")
                return iter(())
        points = Sequence()
        self.assertIsNone(editing._empty_point_array_shape(points))
        self.assertEqual(calls, [])

    def test_staged_mutation_rejects_before_shape_inspection(self):
        with patch("_manim_updaters.canonical_callback_phase_active", return_value=True), \
             patch.object(editing, "_empty_point_array_shape") as shape:
            with self.assertRaises(NotImplementedError):
                editing.set_points(object(), [])
        shape.assert_not_called()

    def test_public_reads_preserve_native_shape_with_index_only_arrays(self):
        try:
            import numpy as np
        except ImportError:
            self.skipTest("NumPy result shapes")
        class IndexOnly:
            def __init__(self, values):
                self.values = values
            def __len__(self):
                return len(self.values)
            def __getitem__(self, index):
                if type(index) is not int:
                    raise AssertionError("array slices are not available")
                return self.values[index]
        value = identity_only_wrapper(compat.Mobject)
        for shape in ((0,), (0, 2), (0, 3), (0, 2, 3)):
            array = IndexOnly((IndexOnly(()), IndexOnly(shape)))
            with patch.object(queries, "_point_array_observation", return_value=array):
                for result in (value.get_all_points(), value.get_merged_array("points")):
                    self.assertIsInstance(result, np.ndarray)
                    self.assertEqual(result.shape, shape)
                    self.assertEqual(result.dtype, np.float64)

    def test_empty_shape_fallback_without_numpy(self):
        for shape in ((0,), (0, 2), (0, 3), (0, 2, 3)):
            with patch.dict(sys.modules, {"numpy": None}), \
                 patch.object(queries, "_point_array_observation", return_value=((), shape)):
                self.assertEqual(queries.all_points(object()), [])

    def test_native_shape_error_is_exposed_as_exact_public_value_error(self):
        message = "all the input arrays must have same number of dimensions, but the array at index 0 has 2 dimension(s) and the array at index 1 has 1 dimension(s)"
        error = NoonValueError(NoonErrorCause("invalid_input", "point.array_shape", message), object(), None)
        with patch.object(queries, "_point_array_observation", side_effect=error):
            for method in (lambda: queries.all_points(object()),
                           lambda: queries.merged_array(object(), "points")):
                with self.assertRaises(ValueError) as raised:
                    method()
                self.assertIs(type(raised.exception), ValueError)
                self.assertEqual(str(raised.exception), message)
                self.assertIs(raised.exception.__cause__, error)

    def test_other_native_failures_retain_original_exception_identity(self):
        error = NoonValueError(NoonErrorCause("invalid_input", "another.code", "unchanged"), object(), None)
        with patch.object(queries, "_point_array_observation", side_effect=error):
            with self.assertRaises(NoonValueError) as raised:
                queries.all_points(object())
        self.assertIs(raised.exception, error)
