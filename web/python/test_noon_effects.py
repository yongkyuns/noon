"""Facade routing tests only; actual Rust scene/transaction tests live in Rust."""
import copy
import sys
import types
import unittest
from unittest.mock import Mock, patch

import noon
import _noon_effects as effects
import _manim_semantic_handles as handles
import _manim_updaters as updaters
from _manim_animate import _AlignedAnimationBuilder


class EffectFacadeTests(unittest.TestCase):
    def setUp(self):
        self.request = Mock()
        self.definition = types.SimpleNamespace(
            radius=12.0, pixels=True, intensity=0.25, source="painted",
            red=0.25, green=0.5, blue=1.0, alpha=1.0,
        )
        self.make_update = Mock(return_value=self.request)
        self.make_glow = Mock(return_value=self.definition)
        js = types.ModuleType("js")
        js.noonGlowUpdate, js.noonGlow = self.make_update, self.make_glow
        ffi = types.ModuleType("pyodide.ffi")
        ffi.to_js = tuple
        self.enter(patch.dict(sys.modules, {"js": js, "pyodide.ffi": ffi}))
        self.handle, self.context = Mock(), None
        self.object = object.__new__(noon.Mobject)
        self.enter(patch.object(handles, "_handle_for", side_effect=lambda value: self.handle))
        self.enter(patch.object(handles, "_is_shared_family", return_value=False))
        self.enter(patch.object(handles, "_live_mutation_context", side_effect=lambda value: self.context))
        token = updaters._ACTIVE_CANONICAL_CONTEXT.set(None)
        self.addCleanup(updaters._ACTIVE_CANONICAL_CONTEXT.reset, token)

    def enter(self, context):
        value = context.__enter__()
        self.addCleanup(context.__exit__, None, None, None)
        return value

    def test_constructor_defers_defaults_and_validation_to_rust(self):
        glow = noon.Glow()
        self.make_update.assert_called_once_with((), None, False, None, None)
        self.make_glow.assert_called_once_with(self.request)
        self.request.free.assert_called_once_with()
        self.assertEqual(glow.radius, noon.Pixels(12.0))
        self.assertEqual(glow.intensity, 0.25)
        self.assertEqual(glow.source, "painted")
        self.assertEqual(glow.color.alpha, 1.0)
        self.assertIs(copy.copy(glow), glow)
        self.assertIs(copy.deepcopy(glow), glow)
        with self.assertRaises(AttributeError):
            glow.intensity = 1.0

    def test_setter_returns_receiver_and_passes_only_explicit_fields(self):
        self.assertIs(self.object.set_glow(intensity=1.4), self.object)
        self.make_update.assert_called_once_with((), None, False, 1.4, None)
        self.handle.setGlow.assert_called_once_with(self.request)
        self.request.free.assert_called_once_with()

    def test_pixels_and_color_are_argument_conversion_not_state(self):
        self.object.set_glow(color="#FF0000", radius=noon.Pixels(12), source="silhouette")
        self.make_update.assert_called_once_with((1.0, 0.0, 0.0, 1.0), 12.0, True, None, "silhouette")

    def test_invalid_python_types_and_unknown_names_never_publish(self):
        for field, value in [("radius", True), ("intensity", False), ("intensity", "1"), ("source", 1), ("radius", noon.Pixels(True))]:
            with self.subTest(field=field, value=value), self.assertRaises(TypeError):
                self.object.set_glow(**{field: value})
        with self.assertRaises(TypeError):
            self.object.set_glow(unknown=1)
        self.make_update.assert_not_called()
        self.handle.setGlow.assert_not_called()

    def test_nonfinite_validation_is_not_duplicated_in_python(self):
        error = ValueError("shared Rust parameter rejection")
        self.make_update.side_effect = error
        with self.assertRaises(ValueError) as failure:
            self.object.set_glow(intensity=float("nan"))
        self.assertIs(failure.exception, error)
        self.handle.setGlow.assert_not_called()
        # Construction did not return a Rust-owned request to release.
        self.request.free.assert_not_called()

    def test_update_is_released_on_shared_failure_without_local_rollback(self):
        failure = ValueError("existing Rust transaction rejected the request")
        self.handle.setGlow.side_effect = failure
        with self.assertRaises(ValueError) as caught:
            self.object.set_glow(intensity=1.4)
        self.assertIs(caught.exception, failure)
        self.request.free.assert_called_once_with()
        self.assertFalse(hasattr(self.object, "_effects"))

    def test_names_and_handles_have_distinct_checked_boundary_calls(self):
        rust_handle = Mock()
        rust_handle.authoredDefinition.return_value = self.definition
        self.handle.getEffect.return_value = rust_handle
        bound = self.object.get_effect("accent")
        self.assertEqual(bound.authored_definition.intensity, 0.25)
        self.assertIs(copy.copy(bound), bound)
        self.assertIs(copy.deepcopy(bound), bound)
        self.assertIs(self.object.set_effect(bound, intensity=0.6), self.object)
        self.handle.setEffectHandle.assert_called_once_with(rust_handle, self.request)
        self.assertIs(self.object.set_effect("accent", radius=0.15), self.object)
        self.handle.setEffect.assert_called_once_with("accent", self.request)
        self.assertIs(self.object.remove_effect(bound), self.object)
        self.handle.removeEffectHandle.assert_called_once_with(rust_handle)
        self.assertIs(self.object.remove_effect("accent"), self.object)
        self.handle.removeEffect.assert_called_once_with("accent")
        with self.assertRaises(TypeError):
            self.object.set_effect(123, intensity=0.6)
        with self.assertRaises(TypeError):
            noon.EffectHandle()

    def test_bound_handle_reads_rust_each_time_and_preserves_stale_error(self):
        rust_handle = Mock()
        rust_handle.authoredDefinition.return_value = self.definition
        self.handle.getEffect.return_value = rust_handle
        bound = self.object.get_effect("glow")
        bound.authored_definition
        error = ReferenceError("stale Rust generation")
        rust_handle.authoredDefinition.side_effect = error
        with self.assertRaises(ReferenceError) as caught:
            bound.authored_definition
        self.assertIs(caught.exception, error)
        self.assertEqual(rust_handle.authoredDefinition.call_count, 2)

    def test_generic_attachment_is_fluent_not_a_new_animation_target(self):
        glow = noon.Glow(radius=noon.Pixels(12))
        self.assertIs(self.object.add_effect(glow, name="accent"), self.object)
        self.handle.addEffect.assert_called_once_with(self.definition, "accent")
        self.assertIs(self.object.remove_glow(), self.object)
        self.handle.removeGlow.assert_called_once_with()

    def test_live_calls_never_fall_through_to_raw_authored_mutation(self):
        self.context = Mock()
        self.assertIs(self.object.set_glow(intensity=1.4), self.object)
        self.context.liveSetGlow.assert_called_once_with(self.handle, self.request)
        self.handle.setGlow.assert_not_called()
        self.context.liveSetEffect.side_effect = RuntimeError("transferred player")
        with self.assertRaises(RuntimeError):
            self.object.set_effect("accent", intensity=0.6)
        self.handle.setEffect.assert_not_called()
        self.assertIs(self.object.remove_glow(), self.object)
        self.context.liveRemoveGlow.assert_called_once_with(self.handle)
        self.handle.removeGlow.assert_not_called()

    def test_scope_and_callback_rejection_do_not_walk_or_publish(self):
        glow = noon.Glow()
        for scope in ("each", "composed", "view", "family", "unknown"):
            with self.subTest(scope=scope), self.assertRaises(NotImplementedError):
                self.object.add_effect(glow, name="accent", scope=scope)
        with patch.object(handles, "_is_shared_family", return_value=True):
            with self.assertRaises(NotImplementedError):
                self.object.set_glow(intensity=1.4)
        token = updaters._ACTIVE_CANONICAL_CONTEXT.set(object())
        try:
            with self.assertRaises(NotImplementedError):
                self.object.remove_glow()
        finally:
            updaters._ACTIVE_CANONICAL_CONTEXT.reset(token)
        self.handle.addEffect.assert_not_called()
        self.handle.setGlow.assert_not_called()
        self.handle.removeGlow.assert_not_called()

    def test_existing_animate_builder_composes_motion_and_effect_updates(self):
        # Only boundary routing is mocked. Do not simulate copy/interpolation in Python.
        target = object.__new__(noon.Mobject)
        source = Mock()
        source._copy_for_animate_target.return_value = target
        builder = _AlignedAnimationBuilder(source)
        with patch.object(noon.Mobject, "shift", return_value=target) as shift:
            result = builder(run_time=1.5).shift(noon.RIGHT * 2).set_glow(intensity=1.4).set_effect("accent", intensity=0.6)
        self.assertIs(result, builder)
        self.assertIs(builder.target, target)
        self.assertIs(builder.source, source)
        self.assertEqual(builder.anim_args, {"run_time": 1.5})
        shift.assert_called_once_with(noon.RIGHT * 2)
        self.handle.setGlow.assert_called_once_with(self.request)
        self.handle.setEffect.assert_called_once_with("accent", self.request)
        with self.assertRaises(ValueError):
            builder(run_time=2.0)


if __name__ == "__main__":
    unittest.main()
