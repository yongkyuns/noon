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
