import sys
import types
import unittest
from unittest.mock import Mock, patch
import _manim_compat as compat
import _manim_path_editing as editing
from _typed_geometry_test_support import identity_only_wrapper


class PathEditingTests(unittest.TestCase):
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
                if context is None:
                    handle.setPointsAsCorners.assert_called_once_with([1., 2., 3., 4.])
                else:
                    context.liveSetPointsAsCorners.assert_called_once_with(handle, [1., 2., 3., 4.])

    def test_callback_resource_edits_reject_before_crossing_the_bridge(self):
        value = identity_only_wrapper(compat.VMobject)
        with patch('_manim_updaters.canonical_callback_phase_active', return_value=True), \
             patch.object(editing, '_handle_for') as resolve:
            with self.assertRaisesRegex(NotImplementedError, 'transient resource'):
                value.set_points_as_corners([(1, 2), (3, 4)])
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
            value.close_path()
        handle.startNewPath.assert_called_once_with(1., 2.)
        handle.addLineTo.assert_called_once_with(3., 4.)
        handle.addQuadraticBezierCurveTo.assert_called_once_with(5., 6., 7., 8.)
        handle.addCubicBezierCurveTo.assert_called_once_with(1., 2., 3., 4., 5., 6.)
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
