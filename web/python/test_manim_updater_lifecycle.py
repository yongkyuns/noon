"""Callable bookkeeping delegates interval changes to the shared context."""

import gc
import unittest
import weakref
from types import SimpleNamespace
from unittest.mock import patch

import _manim_updaters as updaters


class RecordingContext:
    def __init__(self):
        self.calls = []
        self.reject = False

    def addUpdater(self, handle, callback, time, position):
        if self.reject:
            raise ValueError("shared transaction rejected registration")
        self.calls.append(("add", handle, callback, time, position))

    def removeUpdater(self, handle, callback, time):
        self.calls.append(("remove", handle, callback, time))

    def clearUpdaters(self, handle, time):
        self.calls.append(("clear", handle, time))


class UpdaterLifecycleTests(unittest.TestCase):
    def setUp(self):
        self.context = RecordingContext()
        self.scene = SimpleNamespace(time=0.0, _canonical_authoring_context=self.context)
        self.handle = SimpleNamespace(semanticSlot=7, semanticGeneration=3)
        # Exercise the actual Python identity wrapper; no engine values are
        # synthesized here because the context below only records registration.
        self.mobject = object.__new__(updaters._base.Mobject)
        self.mobject._scene = None
        self.mobject._object = object()
        self.mobject._semantic_handle = self.handle

    def tearDown(self):
        session = getattr(self.scene, "_noon_canonical_callback_session", None)
        if session is not None:
            updaters.release_session(session.session_id)
        updaters._TRACKED_MOBJECTS.pop(id(self.mobject), None)

    def bind(self):
        self.mobject._scene = self.scene
        updaters.prepare_canonical_callbacks(self.scene, self.context)

    def test_add_remove_and_clear_delegate_authored_boundaries(self):
        forward = lambda mobject, dt: None
        backward = lambda mobject, dt: None
        updaters.add_updater(self.mobject, forward)
        self.bind()
        self.scene.time = 2.0
        updaters.remove_updater(self.mobject, forward)
        updaters.add_updater(self.mobject, backward)
        self.scene.time = 4.0
        updaters.clear_updaters(self.mobject)
        self.assertFalse(updaters.has_updaters(self.mobject))
        self.assertEqual(self.context.calls, [
            ("add", self.handle, "0", 0.0, None),
            ("remove", self.handle, "0", 2.0),
            ("add", self.handle, "1", 2.0, None),
            ("clear", self.handle, 4.0),
        ])

    def test_detached_removed_occurrence_and_repeated_callable_reach_rust(self):
        callback = lambda mobject: None
        updaters.add_updater(self.mobject, callback)
        updaters.remove_updater(self.mobject, callback)
        updaters.add_updater(self.mobject, callback, index=0)
        self.bind()
        self.assertEqual(self.context.calls, [
            ("add", self.handle, "0", 0.0, None),
            ("remove", self.handle, "0", 0.0),
            ("add", self.handle, "0", 0.0, 0),
        ])
        self.assertEqual(updaters.get_updaters(self.mobject), [callback])
        updaters.prepare_canonical_callbacks(self.scene, self.context)
        self.assertEqual(len(self.context.calls), 3, "published occurrences are not replayed")

    def test_failed_shared_registration_does_not_commit_callable_identity(self):
        self.bind()
        callback = lambda mobject: None
        self.context.reject = True
        with self.assertRaisesRegex(ValueError, "shared transaction rejected"):
            updaters.add_updater(self.mobject, callback)
        self.assertEqual(updaters.get_updaters(self.mobject), [])
        session = self.scene._noon_canonical_callback_session
        self.assertEqual(session.callbacks, {})
        self.assertEqual(session.targets, {})
        self.context.reject = False

    def test_composition_callback_reserves_callable_without_python_scheduling(self):
        self.mobject._scene = self.scene
        callback = lambda mobject, dt: None
        callback_id = updaters.reserve_composition_callback(
            self.scene, self.mobject, callback
        )
        session = self.scene._noon_canonical_callback_session
        self.assertEqual(callback_id, 0)
        self.assertIs(session.callbacks[callback_id], callback)
        self.assertEqual(session.targets[(7, 3)], self.mobject)
        self.assertEqual(self.context.calls, [], "Rust composition owns interval scheduling")
        updaters.add_updater(self.mobject, callback)
        self.assertEqual(self.context.calls, [("add", self.handle, "0", 0.0, None)])
        self.assertIs(session.callbacks[0], callback)


class WeakUpdaterDiscoveryTests(unittest.TestCase):
    def make_mobject(self, cls=None):
        value = object.__new__(cls or updaters._base.Mobject)
        value._scene = None
        value._object = None
        value._semantic_handle = None
        return value

    def test_abandoned_detached_registration_does_not_root_its_wrapper(self):
        before = set(updaters._TRACKED_MOBJECTS)
        value = self.make_mobject()
        updaters.add_updater(value, lambda obj, dt: None)
        ref = weakref.ref(value)
        key = id(value)
        self.assertIn(key, updaters._TRACKED_MOBJECTS)
        del value
        gc.collect()
        self.assertIsNone(ref(), "updater discovery must not own a detached wrapper")
        self.assertNotIn(key, updaters._TRACKED_MOBJECTS)
        self.assertTrue(set(updaters._TRACKED_MOBJECTS).issubset(before))

    def test_discovery_does_not_keep_a_callback_capture_cycle_alive(self):
        class Payload:
            pass
        value = self.make_mobject()
        payload = Payload()
        payload_ref, object_ref = weakref.ref(payload), weakref.ref(value)
        # Self/callback cycles are ordinary Python authoring, not Scene roots.
        callback = lambda obj, dt, captured=payload, target=value: None
        updaters.add_updater(value, callback)
        del value, payload, callback
        gc.collect()
        self.assertIsNone(object_ref())
        self.assertIsNone(payload_ref())

    def test_live_wrappers_keep_identity_order_without_hash_or_equality(self):
        class UnhashableMobject(updaters._base.Mobject):
            __hash__ = None
            def __eq__(self, other):
                raise AssertionError("discovery must not call author equality")
        first = self.make_mobject(UnhashableMobject)
        second = self.make_mobject(UnhashableMobject)
        callback = lambda obj: None
        keys = [id(first), id(second)]
        updaters.add_updater(first, callback)
        updaters.add_updater(second, callback)
        updaters.add_updater(first, callback)
        gc.collect()
        self.assertEqual([key for key in updaters._TRACKED_MOBJECTS if key in keys], keys)
        self.assertIs(updaters._TRACKED_MOBJECTS[keys[0]], first)
        self.assertIs(updaters._TRACKED_MOBJECTS[keys[1]], second)
        self.assertEqual(updaters.get_updaters(first), [callback, callback])
        del first, second
        gc.collect()
        self.assertFalse(any(key in updaters._TRACKED_MOBJECTS for key in keys))

    def test_session_owns_target_until_release_even_after_detachment(self):
        context = RecordingContext()
        scene = SimpleNamespace(time=0.0, _canonical_authoring_context=context)
        value = self.make_mobject()
        value._semantic_handle = SimpleNamespace(semanticSlot=71, semanticGeneration=3)
        value._scene = scene
        updaters.add_updater(value, lambda obj, dt: None)
        session = scene._noon_canonical_callback_session
        ref, key = weakref.ref(value), id(value)
        value._scene = None
        del value
        gc.collect()
        self.assertIsNotNone(ref(), "the active callback session still owns its target")
        updaters.release_session(session.session_id)
        gc.collect()
        self.assertIsNone(ref(), "released detached targets must not leak through discovery")
        self.assertNotIn(key, updaters._TRACKED_MOBJECTS)

    def test_one_session_release_preserves_another_sessions_targets(self):
        values, sessions = [], []
        for slot in (80, 81):
            context = RecordingContext()
            scene = SimpleNamespace(time=0.0, _canonical_authoring_context=context)
            value = self.make_mobject()
            value._semantic_handle = SimpleNamespace(semanticSlot=slot, semanticGeneration=3)
            value._scene = scene
            updaters.add_updater(value, lambda obj: None)
            values.append(value)
            sessions.append(scene._noon_canonical_callback_session)
        try:
            updaters.release_session(sessions[0].session_id)
            self.assertNotIn(id(values[0]), updaters._TRACKED_MOBJECTS)
            self.assertIs(updaters._TRACKED_MOBJECTS[id(values[1])], values[1])
            self.assertIs(sessions[1].targets[(81, 3)], values[1])
        finally:
            updaters.release_session(sessions[1].session_id)


class CallbackEntryTests(unittest.IsolatedAsyncioTestCase):
    async def test_run_forwards_frame_and_pinned_player_once(self):
        player = object()
        frame = {"token": {"sequence": 2}, "invocations": [{"callback_id": "7"}]}
        with patch.object(updaters, "run_canonical_callback_phase", return_value="batch") as run:
            result = await updaters._run_canonical_callback_phase_json(
                "3", updaters.json.dumps(frame), player
            )
        self.assertEqual(result, "batch")
        run.assert_called_once_with(3, frame, callback_player=player)

    async def test_completion_and_discard_reach_the_same_phase_identity(self):
        identity = {"token": {"sequence": 2}, "region": 4}
        for committed in (True, False):
            with self.subTest(committed=committed), \
                    patch.object(updaters, "complete_canonical_callback_phase") as complete, \
                    patch.object(updaters, "discard_canonical_callback_phase") as discard:
                await updaters._finish_canonical_callback_phase_json(
                    "3", updaters.json.dumps(identity), committed
                )
                selected, other = (complete, discard) if committed else (discard, complete)
                selected.assert_called_once_with(3, identity)
                other.assert_not_called()

    async def test_invalid_json_never_invokes_the_phase(self):
        with patch.object(updaters, "run_canonical_callback_phase") as run, \
                patch.object(updaters, "complete_canonical_callback_phase") as complete:
            with self.assertRaises(ValueError):
                await updaters._run_canonical_callback_phase_json(3, "invalid", object())
            with self.assertRaises(ValueError):
                await updaters._finish_canonical_callback_phase_json(3, "invalid", True)
            run.assert_not_called()
            complete.assert_not_called()

    async def test_phase_and_stale_completion_errors_propagate_unchanged(self):
        failure = RuntimeError("stale callback token")
        with patch.object(updaters, "run_canonical_callback_phase", side_effect=failure), \
                patch.object(updaters, "complete_canonical_callback_phase", side_effect=failure):
            for entry in (
                updaters._run_canonical_callback_phase_json(3, "{}", object()),
                updaters._finish_canonical_callback_phase_json(3, "{}", True),
            ):
                with self.assertRaises(RuntimeError) as caught:
                    await entry
                self.assertIs(caught.exception, failure)


if __name__ == "__main__":
    unittest.main()
