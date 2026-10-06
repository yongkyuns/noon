"""Callable bookkeeping delegates interval changes to the shared context."""

import unittest
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
        self.mobject = SimpleNamespace(
            _scene=None, _object=object(), _semantic_handle=self.handle,
        )

    def tearDown(self):
        session = getattr(self.scene, "_noon_canonical_callback_session", None)
        if session is not None:
            updaters.release_session(session.session_id)
        updaters._TRACKED_MOBJECTS[:] = [
            item for item in updaters._TRACKED_MOBJECTS if item is not self.mobject
        ]

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
