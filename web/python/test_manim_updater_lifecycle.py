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

    def test_time_based_updater_introspection_matches_explicit_dt_parameter(self):
        def plain(mobject):
            return mobject

        def positional_dt(mobject, dt):
            return dt

        def keyword_only_dt(mobject, *, dt):
            return dt

        def variadic(mobject, *args):
            return args

        callbacks = [plain, positional_dt, keyword_only_dt, variadic]
        for callback in callbacks:
            updaters.add_updater(self.mobject, callback)

        self.assertEqual(
            updaters.get_time_based_updaters(self.mobject),
            [positional_dt, keyword_only_dt],
        )
        self.assertTrue(updaters.has_time_based_updater(self.mobject))
        self.assertFalse(updaters.has_time_based_updater(SimpleNamespace()))

    def test_match_updaters_clears_receiver_family_then_copies_donor_root(self):
        events = []

        class Mobject:
            def __init__(self, name, callbacks=()):
                self.name = name
                self._noon_updaters = list(callbacks)
                self.family = [self]

            def get_family(self):
                return self.family

            @property
            def callbacks(self):
                return self._noon_updaters

            def get_updaters(self):
                events.append(("get", self.name))
                return list(self._noon_updaters)

        old_root = lambda mobject: None
        old_child = lambda mobject: None
        donor_root = lambda mobject: None
        donor_child = lambda mobject: None
        root = Mobject("root", [old_root])
        child = Mobject("child", [old_child])
        root.family.append(child)
        donor = Mobject("donor", [donor_root])
        donor_child_object = Mobject("donor-child", [donor_child])
        donor.family.append(donor_child_object)
        tracked_ids = {id(root), id(child), id(donor), id(donor_child_object)}
        self.addCleanup(
            lambda: updaters._TRACKED_MOBJECTS.__setitem__(
                slice(None),
                [
                    item
                    for item in updaters._TRACKED_MOBJECTS
                    if id(item) not in tracked_ids
                ],
            )
        )

        clear = updaters.clear_updaters
        add = updaters.add_updater

        def record_clear(mobject, recursive=True):
            events.append(("clear", mobject.name, recursive))
            return clear(mobject, recursive)

        def record_add(mobject, updater, *args, **kwargs):
            events.append(("add", mobject.name, updater))
            return add(mobject, updater, *args, **kwargs)

        with patch.object(updaters, "clear_updaters", side_effect=record_clear):
            with patch.object(updaters, "add_updater", side_effect=record_add):
                result = updaters.match_updaters(root, donor)

        self.assertIs(result, root)
        self.assertEqual(root.callbacks, [donor_root])
        self.assertEqual(child.callbacks, [])
        self.assertEqual(donor.callbacks, [donor_root])
        self.assertEqual(donor_child_object.callbacks, [donor_child])
        self.assertEqual(
            events[:2], [("clear", "root", False), ("clear", "child", False)]
        )
        self.assertEqual(events[2], ("get", "donor"))
        self.assertIs(events[3][2], donor_root)

    def test_update_runs_dt_and_plain_callbacks_recursively_unless_suspended(self):
        events = []

        class Mobject:
            def __init__(self, name, callbacks=(), children=()):
                self.name = name
                self._noon_updaters = list(callbacks)
                self.submobjects = list(children)
                self._scene = None
                self.updating_suspended = False

            def get_family(self):
                members = [self]
                for child in self.submobjects:
                    members.extend(child.get_family())
                return members

        def root_updater(mobject, dt):
            events.append((mobject.name, dt))

        def child_updater(mobject):
            events.append((mobject.name, None))

        def suspended_updater(mobject):
            events.append((mobject.name, None))

        leaf = Mobject("leaf", [suspended_updater])
        suspended = Mobject("suspended", [child_updater], [leaf])
        suspended.updating_suspended = True
        child = Mobject("child", [child_updater])
        root = Mobject("root", [root_updater], [child, suspended])

        result = updaters.update(root, dt=0.5)

        self.assertIs(result, root)
        self.assertEqual(events, [("root", 0.5), ("child", None)])
        events.clear()
        updaters.update(root, dt=0.25, recursive=False)
        self.assertEqual(events, [("root", 0.25)])

    def test_update_rejects_scene_bound_targets_before_invoking_callbacks(self):
        calls = []

        class Mobject:
            _scene = object()

            def get_family(self):
                return [self]

        target = Mobject()
        updaters._updaters(target).append(lambda mobject: calls.append(mobject))
        with self.assertRaisesRegex(
            NotImplementedError, "canonical Rust callback phase"
        ):
            updaters.update(target)
        self.assertEqual(calls, [])

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
