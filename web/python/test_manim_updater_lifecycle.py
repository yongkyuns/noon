"""Callable bookkeeping delegates interval changes to the shared context."""

import unittest
from types import SimpleNamespace

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
        updaters.add_updater(self.mobject, callback)
        self.assertEqual(self.context.calls, [("add", self.handle, "0", 0.0, None)])
        self.assertIs(session.callbacks[0], callback)


if __name__ == "__main__":
    unittest.main()
