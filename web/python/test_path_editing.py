import sys
import types
import unittest
from unittest.mock import Mock, call, patch

try:
    import numpy
except ImportError:
    numpy = None

import _manim_compat as compat
import _manim_path_editing as editing
from _typed_geometry_test_support import identity_only_wrapper


class PathEditingTests(unittest.TestCase):
    def test_family_raw_points_and_reset_dispatch_only_the_root_handle(self):
        ffi = types.ModuleType("pyodide.ffi")
        ffi.to_js = lambda values: values
        for cls in (compat.Group, compat.VGroup):
            for live in (False, True):
                with self.subTest(cls=cls, live=live):
                    family = identity_only_wrapper(cls)
                    handle = Mock()
                    family._semantic_family_handle = handle
                    context = Mock() if live else None
                    with patch.dict(sys.modules, {"pyodide.ffi": ffi}), \
                         patch.object(editing, "_handle_for") as leaf, \
                         patch.object(editing, "_live_mutation_context", return_value=context), \
                         patch.object(editing, "_live_constructor_context", return_value=None), \
                         patch("_manim_updaters.canonical_callback_phase_active", return_value=False):
                        if cls is compat.Group:
                            with self.assertWarns(DeprecationWarning):
                                self.assertIs(family.set_points(value=[(0, 0), (1, 2)]), family)
                        else:
                            self.assertIs(family.set_points(points=[(0, 0), (1, 2)]), family)
                        self.assertIs(family.reset_points(), family)
                        leaf.assert_not_called()
                    if live:
                        context.liveSetFamilyPoints3D.assert_called_once_with(handle, [0, 0, 0.0, 1, 2, 0.0])
                        context.liveClearFamilyPoints.assert_called_once_with(handle)
                    else:
                        handle.setPoints3D.assert_called_once_with([0, 0, 0.0, 1, 2, 0.0])
                        handle.clearPoints.assert_called_once_with()

    def test_repeat_points_dispatches_family_and_leaf_operations(self):
        family = identity_only_wrapper(compat.Group)
        family_handle = Mock()
        family._semantic_family_handle = family_handle
        family.family_members_with_points = Mock(return_value=[object()])
        value = identity_only_wrapper(compat.VMobject)
        value_handle = Mock()
        value.family_members_with_points = Mock(return_value=[value])

        with patch("_manim_semantic_handles._group_target_context", return_value=None), \
             patch("_manim_updaters.canonical_callback_phase_active", return_value=False):
            self.assertIs(editing.repeat_points(family, 2), family)
            family_handle.repeatPoints.assert_called_once_with(2)

        live_context = Mock()
        with patch("_manim_semantic_handles._group_target_context", return_value=live_context), \
             patch("_manim_updaters.canonical_callback_phase_active", return_value=False):
            self.assertIs(editing.repeat_points(family, 3), family)
            live_context.liveRepeatFamilyPoints.assert_called_once_with(family_handle, 3)

        with patch.object(editing, "_handle_for", return_value=value_handle), \
             patch.object(editing, "_live_mutation_context", return_value=None), \
             patch.object(editing, "_live_constructor_context", return_value=None), \
             patch("_manim_updaters.canonical_callback_phase_active", return_value=False):
            self.assertIs(editing.repeat_points(value, 2), value)
            value_handle.repeatPoints.assert_called_once_with(2)

    def test_repeat_points_matches_empty_and_nonpositive_point_array_behavior(self):
        empty = identity_only_wrapper(compat.VMobject)
        empty.family_members_with_points = Mock(return_value=[])
        with patch("_manim_updaters.canonical_callback_phase_active", return_value=False):
            self.assertIs(editing.repeat_points(empty, 0), empty)
            self.assertIs(editing.repeat_points(empty, -1), empty)

        value = identity_only_wrapper(compat.VMobject)
        value.family_members_with_points = Mock(return_value=[value])
        with patch("_manim_updaters.canonical_callback_phase_active", return_value=False):
            with self.assertRaisesRegex(TypeError, r"reduce\(\) of empty iterable"):
                editing.repeat_points(value, 0)
            with self.assertRaisesRegex(TypeError, r"reduce\(\) of empty iterable"):
                editing.repeat_points(value, -1)

    def test_reverse_points_dispatches_family_and_leaf_operations(self):
        family = identity_only_wrapper(compat.Group)
        family_handle = Mock()
        family._semantic_family_handle = family_handle
        value = identity_only_wrapper(compat.VMobject)
        value_handle = Mock()

        with patch("_manim_semantic_handles._group_target_context", return_value=None), \
             patch("_manim_updaters.canonical_callback_phase_active", return_value=False):
            self.assertIs(editing.reverse_points(family), family)
            family_handle.reversePoints.assert_called_once_with()

        live_context = Mock()
        with patch("_manim_semantic_handles._group_target_context", return_value=live_context), \
             patch("_manim_updaters.canonical_callback_phase_active", return_value=False):
            self.assertIs(editing.reverse_points(family), family)
            live_context.liveReverseFamilyPoints.assert_called_once_with(family_handle)

        with patch.object(editing, "_handle_for", return_value=value_handle), \
             patch.object(editing, "_live_mutation_context", return_value=None), \
             patch.object(editing, "_live_constructor_context", return_value=None), \
             patch("_manim_updaters.canonical_callback_phase_active", return_value=False):
            self.assertIs(editing.reverse_points(value), value)
            value_handle.reversePoints.assert_called_once_with()

        base = object.__new__(compat.Mobject)
        with patch.object(editing, "_handle_for", return_value=None), \
             patch("_manim_updaters.canonical_callback_phase_active", return_value=False):
            self.assertIs(base.reverse_points(), base)

    def test_base_reset_points_uses_shared_path_clear(self):
        value = identity_only_wrapper(compat.Mobject)
        handle = Mock()
        with patch.object(editing, '_handle_for', return_value=handle), \
             patch.object(editing, '_live_mutation_context', return_value=None), \
             patch.object(editing, '_live_constructor_context', return_value=None):
            self.assertIs(value.reset_points(), value)
        handle.clearPoints.assert_called_once_with()

    def test_authored_and_live_dispatch_preserve_wrapper_identity(self):
        value = identity_only_wrapper(compat.VMobject)
        handle = Mock()
        ffi = types.ModuleType('pyodide.ffi')
        ffi.to_js = lambda values: values
        for context in (None, Mock()):
            with patch.dict(sys.modules, {'pyodide.ffi': ffi}), \
                 patch.object(editing, '_handle_for', return_value=handle), \
                 patch.object(editing, '_live_mutation_context', return_value=context), \
                 patch.object(editing, '_live_constructor_context', return_value=None):
                self.assertIs(value.set_points_as_corners([(1, 2, 0), (3, 4, 0)]), value)
                self.assertIs(value.clear_points(), value)
                if context is None:
                    handle.setPointsAsCorners.assert_called_once_with([1.0, 2.0, 0.0, 3.0, 4.0, 0.0])
                    handle.clearPoints.assert_called_once_with()
                else:
                    context.liveSetPointsAsCorners.assert_called_once_with(handle, [1.0, 2.0, 0.0, 3.0, 4.0, 0.0])
                    context.liveClearPoints.assert_called_once_with(handle)

    def test_set_points_as_corners_rejects_unstable_custom_stride_before_bridge(self):
        value = identity_only_wrapper(compat.VMobject)
        for stride in (2, 5):
            value.n_points_per_cubic_curve = stride
            with patch.object(editing, '_handle_for') as resolve:
                with self.assertRaisesRegex(
                    NotImplementedError, 'supported four-point cubic stride'
                ):
                    value.set_points_as_corners([(0, 0), (1, 0)])
                resolve.assert_not_called()

    def test_set_points_as_corners_preserves_fixed_setter_arity_error_for_custom_samples(self):
        value = identity_only_wrapper(compat.VMobject)
        value._bezier_t_values = [0.0, 0.25, 0.5, 0.75, 1.0]
        with patch.object(editing, '_handle_for') as resolve:
            with self.assertRaisesRegex(TypeError, 'four constructor interpolation samples'):
                value.set_points_as_corners([(0, 0), (1, 0)])
            resolve.assert_not_called()

    def test_set_points_as_corners_forwards_mutated_four_samples_to_authored_or_live_rust(self):
        value = identity_only_wrapper(compat.VMobject)
        value._bezier_t_values = [0.0, 0.25, 0.75, 1.0]
        handle = Mock()
        ffi = types.ModuleType('pyodide.ffi')
        ffi.to_js = lambda values: values
        with patch.dict(sys.modules, {'pyodide.ffi': ffi}), \
             patch.object(editing, '_handle_for', return_value=handle), \
             patch.object(editing, '_live_mutation_context', return_value=None), \
             patch.object(editing, '_live_constructor_context', return_value=None):
            self.assertIs(value.set_points_as_corners([(0, 0), (2, 0)]), value)
        handle.setPointsAsCornersWithSamples.assert_called_once_with(
            [0.0, 0.0, 0.0, 2.0, 0.0, 0.0], [0.0, 0.25, 0.75, 1.0]
        )

        context = Mock()
        with patch.dict(sys.modules, {'pyodide.ffi': ffi}), \
             patch.object(editing, '_handle_for', return_value=handle), \
             patch.object(editing, '_live_mutation_context', return_value=context), \
             patch.object(editing, '_live_constructor_context', return_value=None):
            self.assertIs(value.set_points_as_corners([(0, 0), (2, 0)]), value)
        context.liveSetPointsAsCornersWithSamples.assert_called_once_with(
            handle, [0.0, 0.0, 0.0, 2.0, 0.0, 0.0], [0.0, 0.25, 0.75, 1.0]
        )

    def test_set_points_smoothly_uses_corner_set_then_rust_anchor_rewrite(self):
        value = identity_only_wrapper(compat.VMobject)
        points = [(0, 0), (1, 1), (2, 0)]
        set_corners = Mock(return_value=value)
        make_smooth = Mock(return_value=value)
        value.set_points_as_corners = set_corners
        value.make_smooth = make_smooth

        self.assertIs(value.set_points_smoothly(points), value)

        set_corners.assert_called_once_with(points)
        make_smooth.assert_called_once_with()

    def test_set_points_smoothly_preserves_corner_constructor_guards(self):
        for setup, expected_error in (
            (
                lambda value: setattr(value, "n_points_per_cubic_curve", 5),
                NotImplementedError,
            ),
            (
                lambda value: setattr(value, "_bezier_t_values", [0.0, 0.5, 1.0]),
                TypeError,
            ),
        ):
            value = identity_only_wrapper(compat.VMobject)
            setup(value)
            make_smooth = Mock()
            value.make_smooth = make_smooth
            with self.assertRaises(expected_error):
                value.set_points_smoothly([(0, 0), (1, 1)])
            make_smooth.assert_not_called()

    @unittest.skipIf(numpy is None, "NumPy coercion runs in the pinned qualification lane")
    def test_raw_point_set_and_append_use_authored_or_live_rust_publication(self):
        value = identity_only_wrapper(compat.VMobject)
        handle = Mock()
        ffi = types.ModuleType('pyodide.ffi')
        ffi.to_js = lambda values: values
        with patch.dict(sys.modules, {'pyodide.ffi': ffi}):
            for context in (None, Mock()):
                with patch.object(editing, '_handle_for', return_value=handle), \
                     patch.object(editing, '_live_mutation_context', return_value=context), \
                     patch.object(editing, '_live_constructor_context', return_value=None):
                    self.assertIs(value.set_points([(0, 1), (2, 3, .625)]), value)
                    self.assertIs(value.append_points([(4, 5, -.375)]), value)
                if context is None:
                    handle.setPoints3D.assert_called_once_with([0.0, 1.0, 0.0, 2.0, 3.0, .625])
                    handle.appendPoints3D.assert_called_once_with([4.0, 5.0, -.375])
                else:
                    context.liveSetPoints3D.assert_called_once_with(handle, [0.0, 1.0, 0.0, 2.0, 3.0, .625])
                    context.liveAppendPoints3D.assert_called_once_with(handle, [4.0, 5.0, -.375])
                handle.reset_mock()

    @unittest.skipIf(numpy is None, "NumPy coercion runs in the pinned qualification lane")
    def test_raw_point_mutations_are_stride_independent(self):
        value = identity_only_wrapper(compat.VMobject)
        value.n_points_per_cubic_curve = 5
        handle = Mock()
        ffi = types.ModuleType('pyodide.ffi')
        ffi.to_js = lambda values: values
        with patch.dict(sys.modules, {'pyodide.ffi': ffi}), \
             patch.object(editing, '_handle_for', return_value=handle), \
             patch.object(editing, '_live_mutation_context', return_value=None), \
             patch.object(editing, '_live_constructor_context', return_value=None):
            self.assertIs(value.set_points([(0, 1), (2, 3), (4, 5)]), value)
            self.assertIs(value.append_points([(6, 7, 0)]), value)
        handle.setPoints3D.assert_called_once_with([0.0, 1.0, 0.0, 2.0, 3.0, 0.0, 4.0, 5.0, 0.0])
        handle.appendPoints3D.assert_called_once_with([6.0, 7.0, 0.0])

    def test_start_new_path_routes_the_mutable_stride_to_shared_rust(self):
        value = identity_only_wrapper(compat.VMobject)
        value.n_points_per_cubic_curve = 5
        handle = Mock()
        ffi = types.ModuleType('pyodide.ffi')
        ffi.to_js = lambda values: values
        for context in (None, Mock()):
            with patch.dict(sys.modules, {'pyodide.ffi': ffi}), \
                 patch.object(editing, '_handle_for', return_value=handle), \
                 patch.object(editing, '_live_mutation_context', return_value=context), \
                 patch.object(editing, '_live_constructor_context', return_value=None):
                self.assertIs(value.start_new_path((1, 2, 0)), value)
            if context is None:
                handle.startNewPathWithStride.assert_called_once_with(1., 2., 0.0, 5)
            else:
                context.liveStartNewPathWithStride.assert_called_once_with(
                    handle, 1., 2., 0.0, 5
                )
            handle.reset_mock()

    def test_start_new_path_zero_stride_matches_python_modulo_error(self):
        value = identity_only_wrapper(compat.VMobject)
        value.n_points_per_cubic_curve = 0
        with self.assertRaisesRegex(ZeroDivisionError, "integer modulo by zero"):
            value.start_new_path((1, 2, 0))

    def test_scalar_curve_appends_forward_mutable_stride_to_authored_and_live_rust(self):
        value = identity_only_wrapper(compat.VMobject)
        value.n_points_per_cubic_curve = 5
        handle = Mock()
        ffi = types.ModuleType('pyodide.ffi')
        ffi.to_js = lambda values: values
        cases = [
            (value.add_line_to, ((1, 2, 0),), 'addLineTo', 'liveAddLineTo', [1., 2., 0., 5]),
            (
                value.add_quadratic_bezier_curve_to,
                ((1, 2, 0), (3, 4, 0)),
                'addQuadraticBezierCurveTo',
                'liveAddQuadraticBezierCurveTo',
                [1., 2., 0., 3., 4., 0., 5],
            ),
            (
                value.add_cubic_bezier_curve_to,
                ((1, 2, 0), (3, 4, 0), (5, 6, 0)),
                'addCubicBezierCurveTo',
                'liveAddCubicBezierCurveTo',
                [1., 2., 0., 3., 4., 0., 5., 6., 0., 5],
            ),
            (
                value.add_smooth_curve_to,
                ((1, 2, 0),),
                'addSmoothCurveTo',
                'liveAddSmoothCurveTo',
                [[1., 2.], 5],
            ),
            (
                value.add_smooth_curve_to,
                ((1, 2, 0), (3, 4, 0)),
                'addSmoothCurveTo',
                'liveAddSmoothCurveTo',
                [[1., 2., 3., 4.], 5],
            ),
        ]
        with patch.dict(sys.modules, {'pyodide.ffi': ffi}):
            for call, points, authored_method, live_method, expected in cases:
                for context in (None, Mock()):
                    with self.subTest(method=authored_method, live=context is not None), \
                         patch.object(editing, '_handle_for', return_value=handle), \
                         patch.object(editing, '_live_mutation_context', return_value=context), \
                         patch.object(editing, '_live_constructor_context', return_value=None):
                        self.assertIs(call(*points), value)
                    if context is None:
                        getattr(handle, authored_method).assert_called_once_with(*expected)
                    else:
                        getattr(context, live_method).assert_called_once_with(handle, *expected)
                    handle.reset_mock()

    def test_line_append_forwards_custom_constructor_samples_to_authored_and_live_rust(self):
        value = identity_only_wrapper(compat.VMobject)
        value._bezier_t_values = [0.0, 0.25, 0.75, 1.0]
        handle = Mock()
        ffi = types.ModuleType('pyodide.ffi')
        ffi.to_js = lambda values: values
        for context in (None, Mock()):
            with patch.dict(sys.modules, {'pyodide.ffi': ffi}), \
                 patch.object(editing, '_handle_for', return_value=handle), \
                 patch.object(editing, '_live_mutation_context', return_value=context), \
                 patch.object(editing, '_live_constructor_context', return_value=None):
                self.assertIs(value.add_line_to((9, -1, 0)), value)
            if context is None:
                handle.addLineToWithSamples.assert_called_once_with(
                    9.0, -1.0, 0.0, 4, [0.0, 0.25, 0.75, 1.0]
                )
            else:
                context.liveAddLineToWithSamples.assert_called_once_with(
                    handle, 9.0, -1.0, 0.0, 4, [0.0, 0.25, 0.75, 1.0]
                )
            handle.reset_mock()

    def test_line_append_preserves_constructor_sample_arity_errors(self):
        cases = [
            ([0., .5, 1.], "VMobject.add_cubic_bezier_curve_to() missing 1 required positional argument: 'anchor'"),
            ([0., .25, .5, .75, 1.], "VMobject.add_cubic_bezier_curve_to() takes 4 positional arguments but 5 were given"),
        ]
        for samples, expected_error in cases:
            value = identity_only_wrapper(compat.VMobject)
            value._bezier_t_values = samples
            with self.subTest(sample_count=len(samples)):
                with self.assertRaises(TypeError) as captured:
                    value.add_line_to((1, 2, 0))
                self.assertEqual(str(captured.exception), expected_error)

    def test_custom_stride_corners_forward_stride_and_constructor_samples(self):
        value = identity_only_wrapper(compat.VMobject)
        value.n_points_per_cubic_curve = 5
        value._bezier_t_values = [0., 1 / 3, 2 / 3, 1.]
        handle = Mock()
        ffi = types.ModuleType('pyodide.ffi')
        ffi.to_js = lambda values: values
        corners = [(1, 2, 0), (3, 4, 0)]
        expected_points = [1., 2., 0., 3., 4., 0.]
        expected_samples = [0., 1 / 3, 2 / 3, 1.]
        value.throw_error_if_no_points = Mock()
        with patch.dict(sys.modules, {'pyodide.ffi': ffi}):
            for context in (None, Mock()):
                with patch.object(editing, '_handle_for', return_value=handle), \
                     patch.object(editing, '_live_mutation_context', return_value=context), \
                     patch.object(editing, '_live_constructor_context', return_value=None):
                    self.assertIs(value.add_points_as_corners(corners), value)
                if context is None:
                    handle.addPointsAsCornersWithStride.assert_called_once_with(
                        expected_points, 5, expected_samples
                    )
                else:
                    context.liveAddPointsAsCornersWithStride.assert_called_once_with(
                        handle, expected_points, 5, expected_samples
                    )
                handle.reset_mock()

    def test_custom_stride_corners_reject_uninitialized_source_slots(self):
        value = identity_only_wrapper(compat.VMobject)
        value.n_points_per_cubic_curve = 6
        value._bezier_t_values = [0., 1 / 3, 2 / 3, 1.]
        value.throw_error_if_no_points = Mock()
        with patch.object(editing, '_handle_for') as handle:
            with self.assertRaisesRegex(NotImplementedError, "uninitialized points"):
                value.add_points_as_corners([(1, 2, 0)])
        handle.assert_not_called()

    def test_custom_stride_corners_reject_samples_that_cross_stride_groups(self):
        value = identity_only_wrapper(compat.VMobject)
        value.n_points_per_cubic_curve = 2
        value._bezier_t_values = [0., 1 / 3, 2 / 3, 1.]
        value.throw_error_if_no_points = Mock()
        with patch.object(editing, '_handle_for') as handle:
            with self.assertRaisesRegex(NotImplementedError, "across stride groups"):
                value.add_points_as_corners([(1, 2, 0)])
        handle.assert_not_called()

    @unittest.skipIf(numpy is None, "NumPy resize contracts run in the pinned utility qualification lane")
    def test_resize_points_uses_resize_function_then_shared_setter(self):
        value = identity_only_wrapper(compat.VMobject)
        handle = Mock()
        query = Mock()
        query.pointCount = 2
        query.points3D.return_value = [1., 2., .25, 3., 4., .75]
        ffi = types.ModuleType('pyodide.ffi')
        ffi.to_js = lambda values: values
        with patch.dict(sys.modules, {'pyodide.ffi': ffi}), \
             patch('_manim_path_queries._query', return_value=query), \
             patch.object(editing, '_handle_for', return_value=handle), \
             patch.object(editing, '_live_mutation_context', return_value=None), \
             patch.object(editing, '_live_constructor_context', return_value=None):
            self.assertIs(value.resize_points(3), value)
        handle.setPoints3D.assert_called_once_with([1.0, 2.0, .25, 3.0, 4.0, .75, 1.0, 2.0, .25])
        query.free.assert_called_once_with()

        query.reset_mock()
        query.pointCount = 3
        with patch('_manim_path_queries._query', return_value=query), \
             patch.object(editing, '_handle_for') as resolve:
            self.assertIs(value.resize_points(3), value)
            resolve.assert_not_called()
        query.points.assert_not_called()

    @unittest.skipIf(numpy is None, "NumPy resize contracts run in the pinned utility qualification lane")
    def test_resize_points_gives_custom_functions_the_manim_three_column_array(self):
        # patch.dict restores sys.modules wholesale, so load the extension first.
        value = identity_only_wrapper(compat.VMobject)
        handle = Mock()
        query = Mock()
        query.pointCount = 2
        query.points3D.return_value = [1., 2., .25, 3., 4., .75]
        ffi = types.ModuleType('pyodide.ffi')
        ffi.to_js = lambda values: values
        resize_call = Mock(side_effect=lambda points, _length: points[::-1][:1])
        with patch.dict(sys.modules, {'pyodide.ffi': ffi}), \
             patch('_manim_path_queries._query', return_value=query), \
             patch.object(editing, '_handle_for', return_value=handle), \
             patch.object(editing, '_live_mutation_context', return_value=None), \
             patch.object(editing, '_live_constructor_context', return_value=None):
            self.assertIs(value.resize_points(1, resize_func=resize_call), value)
        points, length = resize_call.call_args.args
        self.assertIsInstance(points, numpy.ndarray)
        self.assertEqual(points.shape, (2, 3))
        self.assertEqual(points[:, 2].tolist(), [.25, .75])
        self.assertEqual(length, 1)
        handle.setPoints3D.assert_called_once_with([3.0, 4.0, .75])

    def test_cubic_curve_dispatch_is_one_authored_or_live_edit(self):
        value = identity_only_wrapper(compat.VMobject)
        handle = Mock()
        for context in (None, Mock()):
            with patch.object(editing, '_handle_for', return_value=handle), \
                 patch.object(editing, '_live_mutation_context', return_value=context), \
                 patch.object(editing, '_live_constructor_context', return_value=None):
                self.assertIs(
                    value.add_cubic_bezier_curve((0, 1), (2, 3), (4, 5), (6, 7)),
                    value,
                )
            if context is None:
                handle.addCubicBezierCurve.assert_called_once_with(
                    0., 1., 0.0, 2., 3., 0.0, 4., 5., 0.0, 6., 7., 0.0
                )
            else:
                context.liveAddCubicBezierCurve.assert_called_once_with(
                    handle, 0., 1., 0.0, 2., 3., 0.0, 4., 5., 0.0, 6., 7., 0.0
                )

    def test_smooth_curve_dispatch_accepts_anchor_or_handle_and_anchor(self):
        value = identity_only_wrapper(compat.VMobject)
        handle = Mock()
        ffi = types.ModuleType('pyodide.ffi')
        ffi.to_js = lambda values: values
        cases = [
            (((1, 2, 0),), [1., 2.]),
            (((1, 2, 0), (3, 4, 0)), [1., 2., 3., 4.]),
        ]
        for points, expected in cases:
            for context in (None, Mock()):
                with patch.dict(sys.modules, {'pyodide.ffi': ffi}), \
                     patch.object(editing, '_handle_for', return_value=handle), \
                     patch.object(editing, '_live_mutation_context', return_value=context), \
                     patch.object(editing, '_live_constructor_context', return_value=None):
                    self.assertIs(value.add_smooth_curve_to(*points), value)
                if context is None:
                    self.assertEqual(handle.addSmoothCurveTo.call_args.args, (expected,))
                else:
                    self.assertEqual(
                        context.liveAddSmoothCurveTo.call_args.args,
                        (handle, expected),
                    )
                handle.reset_mock()

    def test_smooth_curve_rejects_invalid_arity_with_manim_error(self):
        value = identity_only_wrapper(compat.VMobject)
        for points in ((), ((1, 2, 0), (3, 4, 0), (5, 6, 0))):
            with self.assertRaisesRegex(
                ValueError,
                r"^Only call add_smooth_curve_to with 1 or 2 points$",
            ):
                value.add_smooth_curve_to(*points)

    def test_handle_distance_scaling_dispatches_one_authored_or_live_edit(self):
        value = identity_only_wrapper(compat.VMobject)
        handle = Mock()
        for context in (None, Mock()):
            with patch.object(editing, '_handle_for', return_value=handle), \
                 patch.object(editing, '_live_mutation_context', return_value=context), \
                 patch.object(editing, '_live_constructor_context', return_value=None):
                self.assertIs(value.scale_handle_to_anchor_distances(0.5), value)
            if context is None:
                handle.scaleHandleToAnchorDistances.assert_called_once_with(0.5)
            else:
                context.liveScaleHandleToAnchorDistances.assert_called_once_with(handle, 0.5)
            handle.reset_mock()

    def test_handle_distance_scaling_rejects_callback_before_resolving_handle(self):
        value = identity_only_wrapper(compat.VMobject)
        with patch('_manim_updaters.canonical_callback_phase_active', return_value=True), \
             patch.object(editing, '_handle_for') as resolve:
            with self.assertRaisesRegex(NotImplementedError, 'transient resource'):
                value.scale_handle_to_anchor_distances(0.5)
            resolve.assert_not_called()

    def test_handle_scaling_custom_stride_gate_tracks_manim_threshold(self):
        value = identity_only_wrapper(compat.VMobject)
        value.n_points_per_cubic_curve = 5
        value.family_members_with_points = Mock(return_value=[value])
        value.get_num_points = Mock(return_value=4)
        with patch.object(editing, '_handle_for') as resolve:
            self.assertIs(value.scale_handle_to_anchor_distances(0.5), value)
            resolve.assert_not_called()

        value.get_num_points.return_value = 5
        with patch.object(editing, '_handle_for') as resolve:
            with self.assertRaisesRegex(
                NotImplementedError, 'supported four-point cubic stride'
            ):
                value.scale_handle_to_anchor_distances(0.5)
            resolve.assert_not_called()

    @unittest.skipIf(numpy is None, "NumPy coercion runs in the pinned qualification lane")
    def test_cubic_batches_use_public_append_and_ignore_its_return(self):
        value = identity_only_wrapper(compat.VMobject)
        curves = numpy.array([[[0., 0., 0.], [1., 0., 0.], [1., 1., 0.], [2., 1., 0.]]])
        captured = []
        value.append_points = lambda points: captured.append(points) or object()
        self.assertIs(value.add_cubic_bezier_curves(curves), value)
        self.assertEqual(len(captured), 1)
        numpy.testing.assert_array_equal(captured[0], curves.flatten())
        self.assertEqual(captured[0].shape, (12,))

    @unittest.skipIf(numpy is None, "NumPy coercion runs in the pinned qualification lane")
    def test_cubic_batches_preserve_pinned_flatten_and_broadcast_errors(self):
        value = identity_only_wrapper(compat.VMobject)
        curve = [(0, 0, 0), (1, 0, 0), (1, 1, 0), (2, 1, 0)]
        with patch.object(editing, "_handle_for") as resolve:
            with self.assertRaisesRegex(AttributeError, "flatten"):
                value.add_cubic_bezier_curves(curve)
            with self.assertRaisesRegex(ValueError, "broadcast"):
                value.add_cubic_bezier_curves(numpy.array(curve))
            resolve.assert_not_called()

    def test_add_points_as_corners_dispatches_one_authored_or_live_edit(self):
        value = identity_only_wrapper(compat.VMobject)
        handle = Mock()
        ffi = types.ModuleType('pyodide.ffi')
        ffi.to_js = lambda values: values
        points = [(1, 0, 0), (2, 1, 0)]
        for context in (None, Mock()):
            with patch.dict(sys.modules, {'pyodide.ffi': ffi}), \
                 patch.object(editing, '_handle_for', return_value=handle), \
                 patch.object(editing, '_live_mutation_context', return_value=context), \
                 patch.object(editing, '_live_constructor_context', return_value=None):
                self.assertIs(value.add_points_as_corners(points), value)
            if context is None:
                handle.addPointsAsCorners.assert_called_once_with([1.0, 0.0, 0.0, 2.0, 1.0, 0.0])
            else:
                context.liveAddPointsAsCorners.assert_called_once_with(
                    handle, [1.0, 0.0, 0.0, 2.0, 1.0, 0.0]
                )

    @unittest.skipIf(numpy is None, "NumPy coercion runs in the pinned qualification lane")
    def test_add_subpath_groups_four_points_and_keeps_manims_assertion(self):
        value = identity_only_wrapper(compat.VMobject)
        handle = Mock()
        ffi = types.ModuleType('pyodide.ffi')
        ffi.to_js = lambda values: values
        points = [
            (0, 0, 0), (1, 0, 0), (1, 1, 0), (2, 1, 0),
            (2, 1, 0), (3, 1, 0), (3, 2, 0), (4, 2, 0),
        ]
        with patch.dict(sys.modules, {'pyodide.ffi': ffi}), \
             patch.object(editing, '_handle_for', return_value=handle), \
             patch.object(editing, '_live_mutation_context', return_value=None), \
             patch.object(editing, '_live_constructor_context', return_value=None):
            self.assertIs(value.add_subpath(points), value)
            handle.appendPoints3D.assert_called_once_with(
                [0., 0., 0., 1., 0., 0., 1., 1., 0., 2., 1., 0., 2., 1., 0., 3., 1., 0., 3., 2., 0., 4., 2., 0.]
            )

        with patch.object(editing, '_handle_for') as resolve:
            with self.assertRaises(AssertionError):
                value.add_subpath(points[:-1])
            resolve.assert_not_called()

    def test_set_anchors_and_handles_interleaves_and_replaces_through_one_edit(self):
        value = identity_only_wrapper(compat.VMobject)
        handle = Mock()
        ffi = types.ModuleType('pyodide.ffi')
        ffi.to_js = lambda values: values
        arrays = (
            [(0, 0, 0), (2, 1, 0)],
            [(1, 0, 0), (3, 1, 0)],
            [(1, 1, 0), (3, 2, 0)],
            [(2, 1, 0), (4, 2, 0)],
        )
        expected = [0., 0., 0., 1., 0., 0., 1., 1., 0., 2., 1., 0., 2., 1., 0., 3., 1., 0., 3., 2., 0., 4., 2., 0.]
        for context in (None, Mock()):
            with patch.dict(sys.modules, {'pyodide.ffi': ffi}), \
                 patch.object(editing, '_handle_for', return_value=handle), \
                 patch.object(editing, '_live_mutation_context', return_value=context), \
                 patch.object(editing, '_live_constructor_context', return_value=None):
                self.assertIs(value.set_anchors_and_handles(*arrays), value)
            if context is None:
                handle.setCubicBezierCurves.assert_called_once_with(expected)
            else:
                context.liveSetCubicBezierCurves.assert_called_once_with(handle, expected)

        with patch.object(editing, '_handle_for') as resolve:
            with self.assertRaises(AssertionError):
                value.set_anchors_and_handles(arrays[0], arrays[1], arrays[2], arrays[3][:-1])
            resolve.assert_not_called()

    def test_set_anchors_and_handles_rejects_unstable_custom_stride(self):
        value = identity_only_wrapper(compat.VMobject)
        arrays = ([(0, 0)], [(1, 0)], [(1, 1)], [(2, 1)])
        for stride in (2, 5):
            value.n_points_per_cubic_curve = stride
            with patch.object(editing, '_handle_for') as resolve:
                with self.assertRaisesRegex(NotImplementedError, 'four-point cubic stride'):
                    value.set_anchors_and_handles(*arrays)
                resolve.assert_not_called()

    def test_append_vectorized_mobject_dispatches_shared_source_in_authored_or_live_mode(self):
        value = identity_only_wrapper(compat.VMobject)
        source = identity_only_wrapper(compat.VMobject)
        handle, source_handle = Mock(), Mock()
        with patch.object(editing, '_handle_for', side_effect=lambda obj: handle if obj is value else source_handle), \
             patch.object(editing, '_live_mutation_context', return_value=None), \
             patch.object(editing, '_live_constructor_context', return_value=None):
            self.assertIs(value.append_vectorized_mobject(source), value)
        handle.appendVectorizedMobject.assert_called_once_with(source_handle)

        context = Mock()
        with patch.object(editing, '_handle_for', side_effect=lambda obj: handle if obj is value else source_handle), \
             patch.object(editing, '_live_mutation_context', side_effect=lambda obj: context if obj is source else None), \
             patch.object(editing, '_live_constructor_context', return_value=None):
            self.assertIs(value.append_vectorized_mobject(source), value)
        context.liveAppendVectorizedMobject.assert_called_once_with(handle, source_handle)

    def test_append_vectorized_mobject_forwards_mutable_stride_in_authored_or_live_mode(self):
        value = identity_only_wrapper(compat.VMobject)
        value.n_points_per_cubic_curve = 5
        source = identity_only_wrapper(compat.VMobject)
        handle, source_handle = Mock(), Mock()
        with patch.object(editing, '_handle_for', side_effect=lambda obj: handle if obj is value else source_handle), \
             patch.object(editing, '_live_mutation_context', return_value=None), \
             patch.object(editing, '_live_constructor_context', return_value=None):
            self.assertIs(value.append_vectorized_mobject(source), value)
        handle.appendVectorizedMobjectWithStride.assert_called_once_with(source_handle, 5)

        context = Mock()
        with patch.object(editing, '_handle_for', side_effect=lambda obj: handle if obj is value else source_handle), \
             patch.object(editing, '_live_mutation_context', side_effect=lambda obj: context if obj is source else None), \
             patch.object(editing, '_live_constructor_context', return_value=None):
            self.assertIs(value.append_vectorized_mobject(source), value)
        context.liveAppendVectorizedMobjectWithStride.assert_called_once_with(
            handle, source_handle, 5
        )

    def test_append_vectorized_mobject_rejects_nonvector_source_and_callback_before_bridge(self):
        value = identity_only_wrapper(compat.VMobject)
        with patch.object(editing, '_handle_for') as resolve:
            with self.assertRaises(TypeError):
                value.append_vectorized_mobject(object())
            with patch('_manim_updaters.canonical_callback_phase_active', return_value=True):
                with self.assertRaisesRegex(NotImplementedError, 'transient resource'):
                    value.append_vectorized_mobject(identity_only_wrapper(compat.VMobject))
            resolve.assert_not_called()

    def test_close_path_forwards_stride_tolerance_and_samples_to_authored_or_live_rust(self):
        value = identity_only_wrapper(compat.VMobject)
        value.n_points_per_cubic_curve = 5
        value.tolerance_for_point_equality = 2e-5
        value._bezier_t_values = [0.0, 1 / 3, 2 / 3, 1.0]
        handle = Mock()
        ffi = types.ModuleType('pyodide.ffi')
        ffi.to_js = lambda values: values
        with patch.dict(sys.modules, {'pyodide.ffi': ffi}), \
             patch.object(editing, '_handle_for', return_value=handle), \
             patch.object(editing, '_live_mutation_context', return_value=None), \
             patch.object(editing, '_live_constructor_context', return_value=None), \
             patch('_manim_path_queries.is_closed', return_value=False):
            self.assertIs(value.close_path(), value)
        authored_args = handle.closePathWithStride.call_args.args
        self.assertEqual(authored_args[:2], (5, 2e-5))
        self.assertEqual(len(authored_args[2]), 4)

        context = Mock()
        with patch.dict(sys.modules, {'pyodide.ffi': ffi}), \
             patch.object(editing, '_handle_for', return_value=handle), \
             patch.object(editing, '_live_mutation_context', return_value=context), \
             patch.object(editing, '_live_constructor_context', return_value=None), \
             patch('_manim_path_queries.is_closed', return_value=False):
            self.assertIs(value.close_path(), value)
        live_args = context.liveClosePathWithStride.call_args.args
        self.assertEqual(live_args[:3], (handle, 5, 2e-5))
        self.assertEqual(len(live_args[3]), 4)

    def test_close_path_returns_unchanged_for_closed_points_before_resolving_handle(self):
        value = identity_only_wrapper(compat.VMobject)
        with patch('_manim_path_queries.is_closed', return_value=True), \
             patch.object(editing, '_handle_for') as resolve:
            self.assertIs(value.close_path(), value)
            resolve.assert_not_called()

    def test_close_path_rejects_constructor_sample_counts_that_make_manims_call_invalid(self):
        value = identity_only_wrapper(compat.VMobject)
        value._bezier_t_values = [0.0, 0.25, 0.5, 0.75, 1.0]
        with patch('_manim_path_queries.is_closed', return_value=False), \
             patch.object(editing, '_handle_for') as resolve:
            with self.assertRaisesRegex(TypeError, "three constructor interpolation samples"):
                value.close_path()
            resolve.assert_not_called()

    def test_callback_resource_edits_reject_before_crossing_the_bridge(self):
        value = identity_only_wrapper(compat.VMobject)
        with patch('_manim_updaters.canonical_callback_phase_active', return_value=True), \
             patch.object(editing, '_handle_for') as resolve:
            with self.assertRaisesRegex(NotImplementedError, 'transient resource'):
                value.set_points_as_corners([(1, 2), (3, 4)])
            with self.assertRaisesRegex(NotImplementedError, 'transient resource'):
                value.set_points([(1, 2), (3, 4)])
            with self.assertRaisesRegex(NotImplementedError, 'transient resource'):
                value.append_points([(1, 2)])
            with self.assertRaisesRegex(NotImplementedError, 'transient resource'):
                value.resize_points(0)
            with self.assertRaisesRegex(NotImplementedError, 'transient resource'):
                value.clear_points()
            with self.assertRaisesRegex(NotImplementedError, 'transient resource'):
                value.add_cubic_bezier_curves([[(0, 0), (1, 0), (1, 1), (2, 1)]])
            with self.assertRaisesRegex(NotImplementedError, 'transient resource'):
                value.add_points_as_corners([(1, 0), (2, 1)])
            with self.assertRaisesRegex(NotImplementedError, 'transient resource'):
                value.add_subpath([(0, 0), (1, 0), (1, 1), (2, 1)])
            with self.assertRaisesRegex(NotImplementedError, 'transient resource'):
                value.set_anchors_and_handles([(0, 0)], [(1, 0)], [(1, 1)], [(2, 1)])
            with self.assertRaisesRegex(NotImplementedError, 'transient resource'):
                value.append_vectorized_mobject(identity_only_wrapper(compat.VMobject))
            resolve.assert_not_called()


    def test_close_path_projects_the_typed_empty_subpath_error_to_ce_index_error(self):
        from _noon_errors import NoonErrorCause, NoonIndexError
        value = identity_only_wrapper(compat.VMobject)
        value.n_points_per_cubic_curve = 5
        typed = NoonIndexError(NoonErrorCause("index_error", "path.close_without_subpath", "no complete subpath"), object(), "close")
        ffi = types.ModuleType("pyodide.ffi")
        ffi.to_js = lambda values: values
        with patch.dict(sys.modules, {"pyodide.ffi": ffi}), \
             patch("_manim_path_queries.is_closed", return_value=False), \
             patch.object(editing, "edit_points", side_effect=typed):
            with self.assertRaises(IndexError) as raised:
                value.close_path()
        self.assertIs(type(raised.exception), IndexError)
        self.assertEqual(str(raised.exception), "list index out of range")
        self.assertIs(raised.exception.__cause__, typed)

    def test_scalar_point_commands_dispatch_to_shared_operations(self):
        value = identity_only_wrapper(compat.VMobject)
        handle = Mock()
        ffi = types.ModuleType('pyodide.ffi')
        ffi.to_js = lambda values: values
        with patch.dict(sys.modules, {'pyodide.ffi': ffi}), \
             patch.object(editing, '_handle_for', return_value=handle), \
             patch.object(editing, '_live_mutation_context', return_value=None), \
             patch.object(editing, '_live_constructor_context', return_value=None), \
             patch('_manim_path_queries.is_closed', return_value=False):
            self.assertIs(value.start_new_path((1, 2, 0)), value)
            value.add_line_to((3, 4))
            value.add_quadratic_bezier_curve_to((5, 6), (7, 8))
            value.add_cubic_bezier_curve_to((1, 2), (3, 4), (5, 6))
            value.add_cubic_bezier_curve((0, 1), (2, 3), (4, 5), (6, 7))
            value.close_path()
        handle.startNewPath.assert_called_once_with(1., 2., 0.0)
        handle.addLineTo.assert_called_once_with(3., 4., 0.0)
        handle.addQuadraticBezierCurveTo.assert_called_once_with(5., 6., 0.0, 7., 8., 0.0)
        handle.addCubicBezierCurveTo.assert_called_once_with(1., 2., 0.0, 3., 4., 0.0, 5., 6., 0.0)
        handle.addCubicBezierCurve.assert_called_once_with(0., 1., 0.0, 2., 3., 0.0, 4., 5., 0.0, 6., 7., 0.0)
        close_args = handle.closePathWithStride.call_args.args
        self.assertEqual(close_args[:2], (4, 1e-6))
        self.assertEqual(len(close_args[2]), 4)

    def test_partial_and_direction_use_shared_operands_in_authored_and_live_modes(self):
        value = identity_only_wrapper(compat.VMobject)
        source = identity_only_wrapper(compat.VMobject)
        handle, source_handle = Mock(), Mock()
        for context in (None, Mock()):
            with patch.object(editing, '_handle_for', side_effect=lambda obj: handle if obj is value else source_handle), \
                 patch.object(editing, '_live_mutation_context', return_value=context), \
                 patch.object(editing, '_live_constructor_context', return_value=None):
                self.assertIs(value.pointwise_become_partial(source, 0.2, 0.8), value)
                self.assertIs(value.reverse_direction(), value)
                if context is None:
                    handle.pointwiseBecomePartial.assert_called_once_with(source_handle, 0.2, 0.8)
                    handle.reverseDirection.assert_called_once_with()
                else:
                    context.livePointwiseBecomePartial.assert_called_once_with(handle, source_handle, 0.2, 0.8)
                    context.liveReverseDirection.assert_called_once_with(handle)

    def test_partial_rejects_nonvector_source_without_resolving_handles(self):
        value = identity_only_wrapper(compat.VMobject)
        with patch.object(editing, '_handle_for') as resolve:
            with self.assertRaises(TypeError):
                value.pointwise_become_partial(object(), 0.2, 0.8)
            resolve.assert_not_called()

    def test_subdivision_validates_integer_input_and_dispatches_to_shared_edit(self):
        value = identity_only_wrapper(compat.VMobject)
        handle = Mock()
        with patch.object(editing, '_handle_for', return_value=handle), \
             patch.object(editing, '_live_mutation_context', return_value=None), \
             patch.object(editing, '_live_constructor_context', return_value=None):
            self.assertIs(value.insert_n_curves(3), value)
            handle.insertNCurves.assert_called_once_with(3)
            for count in [-1, 2**32]:
                with self.assertRaises(ValueError): value.insert_n_curves(count)
            with self.assertRaises(TypeError): value.insert_n_curves(0.5)

class SmoothingDispatchTests(unittest.TestCase):
    def test_path_and_family_modes_dispatch_without_python_geometry(self):
        import _manim_semantic_handles as handles
        for family in (False, True):
            value = object.__new__(compat.VGroup) if family else identity_only_wrapper(compat.VMobject)
            handle = Mock()
            if family: value._semantic_family_handle = handle
            for context in (None, Mock()):
                with patch.object(editing, '_handle_for', return_value=handle), \
                     patch.object(editing, '_live_mutation_context', return_value=context), \
                     patch.object(editing, '_live_constructor_context', return_value=None), \
                     patch.object(handles, '_group_target_context', return_value=context):
                    self.assertIs(value.make_smooth(), value)
                    self.assertIs(value.make_jagged(), value)
                if context is None:
                    handle.makeSmooth.assert_called_once_with()
                    handle.makeJagged.assert_called_once_with()
                else:
                    method = context.liveChangeFamilyAnchorMode if family else context.liveChangeAnchorMode
                    self.assertEqual(method.call_args_list[0].args, (handle, True))
                    self.assertEqual(method.call_args_list[1].args, (handle, False))
        with patch.object(editing, '_handle_for') as resolve:
            with self.assertRaisesRegex(
                AssertionError,
                r'^mode must be either "jagged" or "smooth"$',
            ):
                value.change_anchor_mode('unknown')
            resolve.assert_not_called()

class AlignmentDispatchTests(unittest.TestCase):
    def test_family_and_mixed_pairs_use_typed_roots_without_leaf_traversal(self):
        for left_cls, right_cls in [
            (compat.VGroup, compat.VGroup),
            (compat.VGroup, compat.VMobject),
            (compat.VMobject, compat.VGroup),
        ]:
            for live in (False, True):
                with self.subTest(left=left_cls, right=right_cls, live=live):
                    a, b = identity_only_wrapper(left_cls), identity_only_wrapper(right_cls)
                    left, right = Mock(), Mock()
                    for value, cls, handle in [(a, left_cls, left), (b, right_cls, right)]:
                        if cls is compat.VGroup:
                            value._semantic_family_handle = handle
                    a.align_rgbas = Mock(return_value=a)
                    context = Mock() if live else None
                    with patch.object(editing, '_handle_for', side_effect=lambda value: left if value is a else right), \
                         patch.object(editing, '_live_mutation_context', return_value=context), \
                         patch.object(editing, '_live_constructor_context', return_value=None), \
                         patch.object(compat, '_leaf_mobjects', side_effect=AssertionError('root-only operation')):
                        self.assertIs(a.align_points(b), a)
                    a.align_rgbas.assert_called_once_with(b)
                    left.layoutAnchor.assert_called_once_with(None)
                    right.layoutAnchor.assert_called_once_with(None)
                    if live:
                        context.liveAlignRootPoints.assert_called_once_with(
                            left.layoutAnchor.return_value, right.layoutAnchor.return_value)
                    else:
                        left.layoutAnchor.return_value.alignPathPoints.assert_called_once_with(
                            right.layoutAnchor.return_value)

    def test_align_rgbas_preserves_supported_solid_paint_profiles(self):
        a, b = [identity_only_wrapper(compat.VMobject) for _ in range(2)]
        a.get_fill_rgbas = Mock(return_value=[[1, 0, 0, 0.25]])
        b.get_fill_rgbas = Mock(return_value=[[0, 0, 1, 0.75]])
        a.get_stroke_rgbas = Mock(return_value=[[0, 1, 0, 0.4]])
        b.get_stroke_rgbas = Mock(return_value=[[1, 1, 0, 0.8]])

        self.assertIs(editing.align_rgbas(a, b), a)
        a.get_fill_rgbas.assert_called_once_with()
        b.get_fill_rgbas.assert_called_once_with()
        a.get_stroke_rgbas.assert_has_calls([
            call(), call(background=True)
        ])
        b.get_stroke_rgbas.assert_has_calls([
            call(), call(background=True)
        ])

    def test_align_rgbas_rejects_unequal_array_lengths_before_path_alignment(self):
        a, b = [identity_only_wrapper(compat.VMobject) for _ in range(2)]
        a.get_fill_rgbas = Mock(return_value=[[1, 0, 0, 1], [0, 1, 0, 1]])
        b.get_fill_rgbas = Mock(return_value=[[0, 0, 1, 1]])
        a.get_stroke_rgbas = Mock(return_value=[[1, 1, 1, 1]])
        b.get_stroke_rgbas = Mock(return_value=[[1, 1, 1, 1]])

        with patch.object(editing, '_handle_for') as resolve:
            with self.assertRaisesRegex(
                NotImplementedError, "unequal per-object arrays"
            ):
                a.align_points(b)
            resolve.assert_not_called()

    def test_pair_is_aligned_by_one_shared_call_in_either_mode(self):
        a, b = [identity_only_wrapper(compat.VMobject) for _ in range(2)]
        a.align_rgbas = Mock(return_value=a)
        left, right = Mock(), Mock()
        for context in (None, Mock()):
            with patch.object(editing, '_handle_for', side_effect=lambda v: left if v is a else right), \
                 patch.object(editing, '_live_mutation_context', side_effect=lambda v: context if v is b else None), \
                 patch.object(editing, '_live_constructor_context', return_value=None):
                self.assertIs(a.align_points(b), a)
                a.align_rgbas.assert_called_with(b)
                if context is None:
                    left.alignPoints.assert_called_once_with(right)
                else:
                    context.liveAlignPoints.assert_called_once_with(left, right)

    def test_invalid_pair_or_callback_rejects_before_resolving_handles(self):
        value = identity_only_wrapper(compat.VMobject)
        with patch.object(editing, '_handle_for') as resolve:
            with self.assertRaises(TypeError): value.align_points(object())
            with patch('_manim_updaters.canonical_callback_phase_active', return_value=True):
                with self.assertRaises(NotImplementedError): value.align_points(value)
            resolve.assert_not_called()
