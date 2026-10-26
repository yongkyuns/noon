"""Language boundary routing; shared Rust/native/browser tests own paint semantics."""
import copy
import sys
import types
import unittest
from unittest.mock import Mock, patch

import noon
import _manim_semantic_handles as handles
import _manim_updaters as updaters
import _noon_paints as paints


class PaintFacadeTests(unittest.TestCase):
    def test_image_paint_uses_existing_resource_handle_and_ordinary_fill_routes(self):
        definition = object()
        make = Mock(return_value=definition)
        js = types.ModuleType("js")
        js.noonImagePaint = make
        ffi = types.ModuleType("pyodide.ffi")
        ffi.to_js = tuple
        image_handle, handle, context = Mock(), Mock(), Mock()
        source = object.__new__(noon.ImageMobject)
        value = object.__new__(noon.Mobject)
        self.assertIn("ImagePaint", noon.__all__)
        self.assertIs(noon.ImagePaint, paints.ImagePaint)
        with patch.dict(sys.modules, {"js": js, "pyodide.ffi": ffi}), \
             patch.object(handles, "_handle_for", side_effect=lambda value: image_handle if value is source else handle), \
             patch.object(handles, "_is_shared_family", return_value=False), \
             patch.object(handles, "_live_mutation_context", return_value=None):
            paint = noon.ImagePaint(source)
            make.assert_called_once_with(
                image_handle, "cover", (0.5, 0.5), "clamp", (0.0, 0.0), (1.0, 1.0), 0.0, None,
            )
            self.assertIs(copy.copy(paint), paint)
            self.assertIs(copy.deepcopy(paint), paint)
            self.assertIs(value.set_fill(paint), value)
            handle.setFillImage.assert_called_once_with(definition, None)
            handle.setFillGradient.assert_not_called()
            with patch.object(handles, "_live_mutation_context", return_value=context):
                self.assertIs(value.set_fill(paint, opacity=0.25), value)
                context.liveSetFillImage.assert_called_once_with(handle, definition, 0.25)
            # Detached animate targets use the same ordinary VMobject fill route.
            updaters._canonical_vmobject_set_fill(value, paint, 0.5)
            self.assertEqual(handle.setFillImage.call_args.args, (definition, 0.5))
            make.reset_mock()
            noon.ImagePaint(source, fit="stretch", alignment=[0.25, 0.75], address="repeat",
                           translation=(-0.125, 0.5), scale=(-2, 3), rotation=0.25, sampling="nearest")
            make.assert_called_once_with(
                image_handle, "stretch", (0.25, 0.75), "repeat", (-0.125, 0.5), (-2.0, 3.0), 0.25, "nearest",
            )

    def test_image_paint_rejects_language_shapes_and_booleans_before_engine_call(self):
        make = Mock()
        js = types.ModuleType("js")
        js.noonImagePaint = make
        ffi = types.ModuleType("pyodide.ffi")
        ffi.to_js = tuple
        source = object.__new__(noon.ImageMobject)
        with patch.dict(sys.modules, {"js": js, "pyodide.ffi": ffi}), \
             patch.object(handles, "_handle_for", return_value=Mock()):
            for invalid in [None, "image.png", object.__new__(noon.Mobject), [[255, 0, 0, 255]]]:
                with self.subTest(source=type(invalid).__name__), self.assertRaises(TypeError):
                    noon.ImagePaint(invalid)
            for options in [
                {"fit": True}, {"address": 1}, {"sampling": False}, {"sampling": 2},
                {"rotation": "0"}, {"rotation": True}, {"alignment": (True, 0.5)},
                {"translation": ("0", 0)}, {"scale": (1, False)}, {"alignment": "00"},
                {"translation": 0}, {"scale": None},
            ]:
                with self.subTest(options=options), self.assertRaises(TypeError):
                    noon.ImagePaint(source, **options)
            for options in [{"alignment": (0.5,)}, {"translation": (0, 0, 0)}, {"scale": []}]:
                with self.subTest(options=options), self.assertRaises(ValueError):
                    noon.ImagePaint(source, **options)
        make.assert_not_called()

    def test_image_paint_leaves_value_validation_and_failures_to_rust(self):
        make = Mock(side_effect=ValueError("shared Rust rejected image mapping"))
        js = types.ModuleType("js")
        js.noonImagePaint = make
        ffi = types.ModuleType("pyodide.ffi")
        ffi.to_js = tuple
        source = object.__new__(noon.ImageMobject)
        handle = Mock()
        with patch.dict(sys.modules, {"js": js, "pyodide.ffi": ffi}), \
             patch.object(handles, "_handle_for", return_value=handle):
            with self.assertRaisesRegex(ValueError, "shared Rust"):
                noon.ImagePaint(source, fit="unsupported", scale=(0, 1), alignment=(2, 0.5))
            make.assert_called_once_with(
                handle, "unsupported", (2.0, 0.5), "clamp", (0.0, 0.0), (0.0, 1.0), 0.0, None,
            )

    def test_image_paint_preserves_callback_family_and_missing_handle_gates(self):
        paint = object.__new__(noon.ImagePaint)
        paint._definition = object()
        value = object.__new__(noon.Mobject)
        handle = Mock()
        with patch.object(handles, "_handle_for", return_value=handle), \
             patch.object(handles, "_is_shared_family", return_value=False), \
             patch.object(handles, "_live_mutation_context", return_value=None):
            token = updaters._ACTIVE_CANONICAL_CONTEXT.set(object())
            try:
                with self.assertRaises(NotImplementedError):
                    value.set_fill(paint)
            finally:
                updaters._ACTIVE_CANONICAL_CONTEXT.reset(token)
            with patch.object(handles, "_is_shared_family", return_value=True):
                with self.assertRaises(NotImplementedError):
                    value.set_fill(paint)
            with patch.object(handles, "_handle_for", return_value=None):
                with self.assertRaises(RuntimeError):
                    value.set_fill(paint)
                with self.assertRaises(RuntimeError):
                    noon.ImagePaint(object.__new__(noon.ImageMobject))
            with self.assertRaises(TypeError):
                value.set_fill(paint, opacity=True)
        handle.setFillImage.assert_not_called()

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
