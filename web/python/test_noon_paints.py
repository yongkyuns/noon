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

    def test_image_values_share_pixels_and_route_cold_live_and_uv_only_edits(self):
        candidate = Mock()
        candidate.copy.return_value = cloned = Mock()
        factory = Mock()
        factory.rgba8.return_value = candidate
        mapping_definition = object()
        js = types.ModuleType("js")
        js.noonImageMapping = Mock(return_value=mapping_definition)
        js.noonImagePaintOptions = factory
        ffi = types.ModuleType("pyodide.ffi")
        ffi.to_js = tuple
        handle, context = Mock(), Mock()
        value = object.__new__(noon.Mobject)
        with patch.dict(sys.modules, {"js": js, "pyodide.ffi": ffi}), \
             patch.object(handles, "_handle_for", return_value=handle), \
             patch.object(handles, "_is_shared_family", return_value=False), \
             patch.object(handles, "_live_mutation_context", return_value=None):
            mapping = noon.ImageMapping(translation=(0.173, -0.231), scale=(-0.7, 1.2))
            paint = noon.ImagePaint([[[17, 73, 191, 128]]], mapping=mapping)
            factory.rgba8.assert_called_once_with(1, 1, (17, 73, 191, 128))
            candidate.setSampling.assert_called_once_with("bilinear")
            candidate.setMapping.assert_called_once_with(mapping_definition)
            self.assertIs(copy.deepcopy(paint), paint)
            self.assertIs(copy.copy(mapping), mapping)
            changed = paint.with_mapping(mapping)
            self.assertIs(changed._definition, cloned)
            cloned.setMapping.assert_called_once_with(mapping_definition)
            value.set_fill(paint, opacity=0.25)
            handle.setFillImage.assert_called_once_with(candidate, 0.25)
            value.set_fill_image_mapping(mapping, opacity=0.75)
            handle.setFillImageMapping.assert_called_once_with(mapping_definition, 0.75)
            with patch.object(handles, "_live_mutation_context", return_value=context):
                value.set_fill(paint)
                context.liveSetFillImage.assert_called_once_with(handle, candidate, None)
                value.set_fill_image_mapping(mapping)
                context.liveSetFillImageMapping.assert_called_once_with(handle, mapping_definition, None)
            updaters._canonical_vmobject_set_fill(value, paint, 0.5)
            self.assertEqual(handle.setFillImage.call_args.args, (candidate, 0.5))
            self.assertEqual(factory.rgba8.call_count, 1, "mapping edits must not transfer pixel bytes")
            with self.assertRaises(TypeError):
                value.set_fill_image_mapping(object())
            with self.assertRaises(TypeError):
                value.set_fill_image_mapping(mapping, opacity=True)
