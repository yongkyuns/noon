import sys
import types
import unittest
from unittest.mock import Mock, patch
import _manim_compat as compat
import _manim_path_editing as editing
from _typed_geometry_test_support import identity_only_wrapper


class PathEditingTests(unittest.TestCase):
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
                    handle.setPointsAsCorners.assert_called_once_with([1., 2., 3., 4.])
                    handle.clearPoints.assert_called_once_with()
                else:
                    context.liveSetPointsAsCorners.assert_called_once_with(handle, [1., 2., 3., 4.])
                    context.liveClearPoints.assert_called_once_with(handle)

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
                    0., 1., 2., 3., 4., 5., 6., 7.
                )
            else:
                context.liveAddCubicBezierCurve.assert_called_once_with(
                    handle, 0., 1., 2., 3., 4., 5., 6., 7.
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

    def test_cubic_curve_batches_flatten_to_one_authored_or_live_edit(self):
        value = identity_only_wrapper(compat.VMobject)
        handle = Mock()
        ffi = types.ModuleType('pyodide.ffi')
        ffi.to_js = lambda values: values
        curves = [
            [(0, 0, 0), (1, 0, 0), (1, 1, 0), (2, 1, 0)],
            [(2, 1, 0), (3, 1, 0), (3, 2, 0), (4, 2, 0)],
        ]
        expected = [0., 0., 1., 0., 1., 1., 2., 1., 2., 1., 3., 1., 3., 2., 4., 2.]
        for context in (None, Mock()):
            with patch.dict(sys.modules, {'pyodide.ffi': ffi}), \
                 patch.object(editing, '_handle_for', return_value=handle), \
                 patch.object(editing, '_live_mutation_context', return_value=context), \
                 patch.object(editing, '_live_constructor_context', return_value=None):
                self.assertIs(value.add_cubic_bezier_curves(curves), value)
            if context is None:
                handle.addCubicBezierCurves.assert_called_once_with(expected)
            else:
                context.liveAddCubicBezierCurves.assert_called_once_with(handle, expected)

    def test_cubic_curve_batch_accepts_one_four_point_curve(self):
        value = identity_only_wrapper(compat.VMobject)
        handle = Mock()
        ffi = types.ModuleType('pyodide.ffi')
        ffi.to_js = lambda values: values
        curve = [(0, 0, 0), (1, 0, 0), (1, 1, 0), (2, 1, 0)]
        with patch.dict(sys.modules, {'pyodide.ffi': ffi}), \
             patch.object(editing, '_handle_for', return_value=handle), \
             patch.object(editing, '_live_mutation_context', return_value=None), \
             patch.object(editing, '_live_constructor_context', return_value=None):
            self.assertIs(value.add_cubic_bezier_curves(curve), value)
        handle.addCubicBezierCurves.assert_called_once_with(
            [0., 0., 1., 0., 1., 1., 2., 1.]
        )

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
                handle.addPointsAsCorners.assert_called_once_with([1., 0., 2., 1.])
            else:
                context.liveAddPointsAsCorners.assert_called_once_with(
                    handle, [1., 0., 2., 1.]
                )

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
            handle.addCubicBezierCurves.assert_called_once_with(
                [0., 0., 1., 0., 1., 1., 2., 1., 2., 1., 3., 1., 3., 2., 4., 2.]
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
        expected = [0., 0., 1., 0., 1., 1., 2., 1., 2., 1., 3., 1., 3., 2., 4., 2.]
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

    def test_append_vectorized_mobject_rejects_nonvector_source_and_callback_before_bridge(self):
        value = identity_only_wrapper(compat.VMobject)
        with patch.object(editing, '_handle_for') as resolve:
            with self.assertRaises(TypeError):
                value.append_vectorized_mobject(object())
            with patch('_manim_updaters.canonical_callback_phase_active', return_value=True):
                with self.assertRaisesRegex(NotImplementedError, 'transient resource'):
                    value.append_vectorized_mobject(identity_only_wrapper(compat.VMobject))
            resolve.assert_not_called()

    def test_callback_resource_edits_reject_before_crossing_the_bridge(self):
        value = identity_only_wrapper(compat.VMobject)
        with patch('_manim_updaters.canonical_callback_phase_active', return_value=True), \
             patch.object(editing, '_handle_for') as resolve:
            with self.assertRaisesRegex(NotImplementedError, 'transient resource'):
                value.set_points_as_corners([(1, 2), (3, 4)])
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


    def test_scalar_point_commands_dispatch_to_shared_operations(self):
        value = identity_only_wrapper(compat.VMobject)
        handle = Mock()
        with patch.object(editing, '_handle_for', return_value=handle), \
             patch.object(editing, '_live_mutation_context', return_value=None), \
             patch.object(editing, '_live_constructor_context', return_value=None):
            self.assertIs(value.start_new_path((1, 2, 0)), value)
            value.add_line_to((3, 4))
            value.add_quadratic_bezier_curve_to((5, 6), (7, 8))
            value.add_cubic_bezier_curve_to((1, 2), (3, 4), (5, 6))
            value.add_cubic_bezier_curve((0, 1), (2, 3), (4, 5), (6, 7))
            value.close_path()
        handle.startNewPath.assert_called_once_with(1., 2.)
        handle.addLineTo.assert_called_once_with(3., 4.)
        handle.addQuadraticBezierCurveTo.assert_called_once_with(5., 6., 7., 8.)
        handle.addCubicBezierCurveTo.assert_called_once_with(1., 2., 3., 4., 5., 6.)
        handle.addCubicBezierCurve.assert_called_once_with(0., 1., 2., 3., 4., 5., 6., 7.)
        handle.closePath.assert_called_once_with()

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
            with self.assertRaises(ValueError): value.change_anchor_mode('unknown')
            resolve.assert_not_called()

class AlignmentDispatchTests(unittest.TestCase):
    def test_pair_is_aligned_by_one_shared_call_in_either_mode(self):
        a, b = [identity_only_wrapper(compat.VMobject) for _ in range(2)]
        left, right = Mock(), Mock()
        for context in (None, Mock()):
            with patch.object(editing, '_handle_for', side_effect=lambda v: left if v is a else right), \
                 patch.object(editing, '_live_mutation_context', side_effect=lambda v: context if v is b else None), \
                 patch.object(editing, '_live_constructor_context', return_value=None):
                self.assertIs(a.align_points(b), a)
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
