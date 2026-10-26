"""Language boundary routing; shared Rust/native/browser tests own paint semantics."""
import copy
import sys
import types
import unittest
from unittest.mock import Mock, patch

import noon
import _manim_semantic_handles as handles
import _manim_updaters as updaters


class PaintFacadeTests(unittest.TestCase):
    def test_immutable_value_and_ordinary_cold_live_and_target_fill_routing(self):
        definition = object()
        make = Mock(return_value=definition)
        js = types.ModuleType("js")
        js.noonLinearGradient = make
        ffi = types.ModuleType("pyodide.ffi")
        ffi.to_js = tuple
        handle, context = Mock(), Mock()
        value = object.__new__(noon.Mobject)
        with patch.dict(sys.modules, {"js": js, "pyodide.ffi": ffi}), \
             patch.object(handles, "_handle_for", return_value=handle), \
             patch.object(handles, "_is_shared_family", return_value=False), \
             patch.object(handles, "_live_mutation_context", return_value=None):
            paint = noon.LinearGradient([(0, noon.Color(1, 0, 0)), (1, noon.Color(0, 0, 1))])
            make.assert_called_once_with((0.0, 0.5), (1.0, 0.5), (0.0, 1, 0, 0, 1, 1.0, 0, 0, 1, 1))
            self.assertIs(copy.copy(paint), paint)
            self.assertIs(copy.deepcopy(paint), paint)
            self.assertIs(value.set_fill(paint), value)
            handle.setFillGradient.assert_called_once_with(definition, None)
            with patch.object(handles, "_live_mutation_context", return_value=context):
                self.assertIs(value.set_fill(paint, opacity=0.25), value)
                context.liveSetFillGradient.assert_called_once_with(handle, definition, 0.25)
            # VMobject uses the same operation, including detached animate targets.
            updaters._canonical_vmobject_set_fill(value, paint, 0.5)
            self.assertEqual(handle.setFillGradient.call_args.args, (definition, 0.5))
            token = updaters._ACTIVE_CANONICAL_CONTEXT.set(object())
            try:
                with self.assertRaises(NotImplementedError):
                    value.set_fill(paint)
            finally:
                updaters._ACTIVE_CANONICAL_CONTEXT.reset(token)
            with self.assertRaises(TypeError):
                value.set_fill(paint, opacity=True)
            with self.assertRaises(TypeError):
                noon.LinearGradient([(True, noon.Color(1, 0, 0)), (1, noon.Color(0, 0, 1))])
