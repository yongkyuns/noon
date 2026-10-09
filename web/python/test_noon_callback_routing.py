"""Callback routing stays invocation-scoped; the empty path inspects no wrappers."""
import unittest
from types import SimpleNamespace
from unittest.mock import patch

import _manim_updaters as callbacks


class UninspectedWrapper:
    def __getattribute__(self, name):
        if name.startswith("_") and name != "__class__":
            raise AssertionError(f"inactive callback routing inspected {name}")
        return object.__getattribute__(self, name)


def phase(scene, owner, token):
    # No engine stand-in: these tests exercise host identity selection only.
    context = object.__new__(callbacks._CanonicalCallbackContext)
    context._scene = scene
    context._authoring_context = owner
    context.token = token
    return context


class CallbackRoutingTests(unittest.TestCase):
    def setUp(self):
        self.contexts = patch.dict(callbacks._ACTIVE_CONTEXTS, {}, clear=True)
        self.contexts.start()
        self.token = callbacks._ACTIVE_CANONICAL_CONTEXT.set(None)

    def tearDown(self):
        callbacks._ACTIVE_CANONICAL_CONTEXT.reset(self.token)
        self.contexts.stop()

    def test_inactive_paths_do_not_inspect_wrapper_state(self):
        wrapper = UninspectedWrapper()
        for _ in range(2048):
            self.assertIsNone(callbacks._canonical_phase_context(wrapper))
            self.assertIsNone(callbacks._canonical_provisional_context(wrapper))

    def test_phase_entry_and_exit_are_not_cached(self):
        scene = object()
        wrapper = SimpleNamespace(_scene=scene, _object=object())
        context = phase(scene, object(), {"sequence": 1})
        self.assertIsNone(callbacks._canonical_phase_context(wrapper))
        callbacks._ACTIVE_CONTEXTS[id(scene)] = context
        self.assertIs(callbacks._canonical_phase_context(wrapper), context)
        callbacks._ACTIVE_CONTEXTS.clear()
        self.assertIsNone(callbacks._canonical_phase_context(wrapper))
        callbacks._ACTIVE_CONTEXTS[id(scene)] = context
        self.assertIs(callbacks._canonical_phase_context(wrapper), context)

    def test_other_scenes_and_detached_wrappers_do_not_acquire_an_overlay(self):
        scene = object()
        callbacks._ACTIVE_CONTEXTS[id(scene)] = phase(scene, object(), {})
        for wrapper in (SimpleNamespace(_scene=None, _object=None),
                        SimpleNamespace(_scene=scene, _object=None),
                        SimpleNamespace(_scene=object(), _object=object())):
            self.assertIsNone(callbacks._canonical_phase_context(wrapper))

    def test_provisional_routing_preserves_scene_owner_and_token_validation(self):
        scene, owner, handle = object(), object(), object()
        created = phase(scene, owner, {"sequence": 3})
        wrapper = SimpleNamespace(_callback_provisional_context=created,
                                  _callback_provisional_handle=handle)
        self.assertIsNone(callbacks._canonical_provisional_context(wrapper))
        current = phase(scene, owner, {"sequence": 3})
        callbacks._ACTIVE_CANONICAL_CONTEXT.set(current)
        self.assertEqual(callbacks._canonical_provisional_context(wrapper), (current, handle))
        for field, other in (("_scene", object()), ("_authoring_context", object()),
                             ("token", {"sequence": 4})):
            previous = getattr(current, field)
            setattr(current, field, other)
            self.assertIsNone(callbacks._canonical_provisional_context(wrapper), field)
            setattr(current, field, previous)
        callbacks._ACTIVE_CANONICAL_CONTEXT.set(None)
        self.assertIsNone(callbacks._canonical_provisional_context(wrapper))
        callbacks._ACTIVE_CANONICAL_CONTEXT.set(current)
        self.assertEqual(callbacks._canonical_provisional_context(wrapper), (current, handle))

    def test_invalid_provisional_markers_are_not_accepted_in_an_active_phase(self):
        scene, owner = object(), object()
        current = phase(scene, owner, {})
        callbacks._ACTIVE_CANONICAL_CONTEXT.set(current)
        for wrapper in (SimpleNamespace(),
                        SimpleNamespace(_callback_provisional_context=current,
                                        _callback_provisional_handle=None),
                        SimpleNamespace(_callback_provisional_context=object(),
                                        _callback_provisional_handle=object())):
            self.assertIsNone(callbacks._canonical_provisional_context(wrapper))


if __name__ == "__main__":
    unittest.main()
