"""Callable bookkeeping delegates interval changes to the shared context."""

import gc
import unittest
import weakref
from types import SimpleNamespace
from unittest.mock import patch

from noon import Mobject
import _manim_compat as compat
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
        self.mobject = object.__new__(updaters._base.Mobject)
        self.mobject._scene = None
        self.mobject._object = object()
        self.mobject._semantic_handle = self.handle

    def tearDown(self):
        session = getattr(self.scene, "_noon_canonical_callback_session", None)
        if session is not None:
            updaters.release_session(session.session_id)
        updaters._TRACKED_MOBJECTS.pop(id(self.mobject), None)
        group = getattr(self, "family_group", None)
        if group is not None:
            updaters._TRACKED_MOBJECTS.pop(id(group), None)

    def bind(self):
        self.mobject._scene = self.scene
        updaters.prepare_canonical_callbacks(self.scene, self.context)

    def test_callback_bootstrap_skips_detached_family_wrappers_without_scene_field(self):
        group = compat.Group.__new__(compat.Group)
        group._semantic_family_handle = object()
        self.family_group = group
        updaters._track(group)

        self.assertIsNone(
            updaters.prepare_canonical_callbacks(self.scene, self.context)
        )

    def test_always_builder_invokes_methods_now_and_registers_each_as_updater(self):
        mobject = Mobject.__new__(Mobject)
        events = []
        registered = []

        def shift(distance, *, axis):
            events.append(("shift", distance, axis))
            return mobject

        def scale(factor):
            events.append(("scale", factor))
            return mobject

        def record_updater(callback, index=None, call_updater=False):
            registered.append(callback)
            if call_updater:
                callback(mobject)
            return mobject

        mobject.shift = shift
        mobject.scale = scale
        mobject.add_updater = record_updater

        builder = mobject.always
        self.assertIs(builder.shift(2.0, axis="x"), builder)
        self.assertIs(builder.scale(0.5), builder)

        self.assertEqual(events, [("shift", 2.0, "x"), ("scale", 0.5)])
        self.assertEqual(len(registered), 2)
        for callback in registered:
            callback(mobject)
        self.assertEqual(
            events,
            [
                ("shift", 2.0, "x"),
                ("scale", 0.5),
                ("shift", 2.0, "x"),
                ("scale", 0.5),
            ],
        )

    def test_immediate_updater_runs_for_detached_rust_handles(self):
        events = []
        callback = lambda mobject: events.append(mobject)

        result = updaters.add_updater(
            self.mobject, callback, call_updater=True
        )

        self.assertIs(result, self.mobject)
        self.assertEqual(events, [self.mobject])
        self.assertEqual(updaters.get_updaters(self.mobject), [callback])

    def test_immediate_updater_is_registered_before_invocation_for_live_handles(self):
        events = []

        def callback(mobject):
            events.append((mobject, updaters.get_updaters(mobject)))

        self.bind()
        result = updaters.add_updater(self.mobject, callback, call_updater=True)

        self.assertIs(result, self.mobject)
        self.assertEqual(updaters.get_updaters(self.mobject), [callback])
        self.assertEqual(events, [(self.mobject, [callback])])
        self.assertEqual(
            self.context.calls,
            [("add", self.handle, "0", 0.0, None)],
        )

    def test_live_immediate_callback_does_not_run_when_shared_registration_fails(self):
        events = []
        callback = lambda mobject: events.append(mobject)
        self.bind()
        self.context.reject = True

        with self.assertRaisesRegex(ValueError, "shared transaction rejected registration"):
            updaters.add_updater(self.mobject, callback, call_updater=True)

        self.assertEqual(events, [])
        self.assertEqual(updaters.get_updaters(self.mobject), [])

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
        for key in tracked_ids:
            self.addCleanup(updaters._TRACKED_MOBJECTS.pop, key, None)

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

    def test_suspension_uses_shared_state_and_resume_matches_pinned_recursion(self):
        import _manim_semantic_handles as semantic

        class SharedStateHandle:
            def __init__(self, owner):
                self.owner = owner

            def updatingSuspended(self):
                return self.owner.suspended

            def setUpdatingSuspended(self, suspended, recursive):
                seen = set()

                def write(member):
                    if id(member) in seen:
                        return
                    seen.add(id(member))
                    member.suspended = suspended
                    if recursive:
                        for child in member.submobjects:
                            write(child)

                write(self.owner)

        class Mobject:
            def __init__(self, name, children=()):
                self.name = name
                self.submobjects = list(children)
                self._scene = None
                self.suspended = False
                self._noon_updaters = []
                self._handle = SharedStateHandle(self)

            def get_family(self):
                members = [self]
                for child in self.submobjects:
                    members.extend(child.get_family())
                return members

            @property
            def updating_suspended(self):
                return updaters.updating_suspended(self)

            @updating_suspended.setter
            def updating_suspended(self, value):
                updaters.set_updating_suspended(self, bool(value), recursive=False)

            def suspend_updating(self, recursive=True):
                return updaters.suspend_updating(self, recursive)

            def resume_updating(self, recursive=True):
                return updaters.resume_updating(self, recursive)

        events = []
        leaf = Mobject("leaf")
        child = Mobject("child", [leaf])
        root = Mobject("root", [child])
        for member in (root, child, leaf):
            member._noon_updaters.append(
                lambda target, member=member: events.append(member.name)
            )

        with patch.object(
            semantic,
            "_style_target",
            side_effect=lambda member: (member._handle, None, ""),
        ):
            self.assertFalse(root.updating_suspended)
            root.updating_suspended = True
            self.assertTrue(root.updating_suspended)
            self.assertFalse(child.updating_suspended)
            root.suspend_updating(recursive=True)
            self.assertTrue(root.updating_suspended)
            self.assertTrue(child.updating_suspended)
            self.assertTrue(leaf.updating_suspended)
            self.assertEqual(events, [])

            self.assertIs(root.resume_updating(), root)
            self.assertFalse(root.updating_suspended)
            self.assertFalse(child.updating_suspended)
            self.assertFalse(leaf.updating_suspended)
            # Manim recursively resumes each child (including its immediate
            # update), then runs update(0) from the root.
            self.assertEqual(events, ["leaf", "child", "leaf", "root", "child", "leaf"])

    def test_recursive_suspension_rejects_scene_owned_descendant_before_mutation(self):
        import _manim_semantic_handles as semantic

        class Handle:
            def __init__(self, owner):
                self.owner = owner
                self.calls = []

            def setUpdatingSuspended(self, suspended, recursive):
                self.calls.append((suspended, recursive))

        class Mobject:
            def __init__(self, children=()):
                self.submobjects = list(children)
                self._scene = None
                self._handle = Handle(self)

            def get_family(self):
                return [self, *(member for child in self.submobjects for member in child.get_family())]

        child = Mobject()
        child._scene = object()
        root = Mobject([child])
        with patch.object(
            semantic,
            "_style_target",
            side_effect=lambda member: (member._handle, None, ""),
        ):
            with self.assertRaisesRegex(NotImplementedError, "Scene-bound"):
                updaters.suspend_updating(root, recursive=True)
        self.assertEqual(root._handle.calls, [])

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


class UpdaterDiscoveryTests(unittest.TestCase):
    def setUp(self):
        # Isolate discovery without prescribing whether its storage owns values.
        registry = type(updaters._TRACKED_MOBJECTS)()
        discovery = patch.object(updaters, "_TRACKED_MOBJECTS", registry)
        discovery.start()
        self.addCleanup(discovery.stop)

    def make_mobject(self, slot=7, cls=None):
        value = object.__new__(cls or updaters._base.Mobject)
        value._scene = None
        value._object = None
        value._semantic_handle = SimpleNamespace(semanticSlot=slot, semanticGeneration=3)
        return value

    def test_abandoned_registration_does_not_retain_wrapper_or_callback_capture(self):
        class Payload:
            pass

        value, payload = self.make_mobject(), Payload()
        object_ref, payload_ref = weakref.ref(value), weakref.ref(payload)
        callback = lambda obj, dt, target=value, captured=payload: None
        updaters.add_updater(value, callback)
        del value, payload, callback
        gc.collect()
        self.assertIsNone(object_ref(), "discovery must not own abandoned wrappers")
        self.assertIsNone(payload_ref(), "callback captures must be collectable")

    def test_identity_discovery_preserves_wrapper_order_and_duplicate_callables(self):
        class UnhashableMobject(updaters._base.Mobject):
            __hash__ = None

            def __eq__(self, other):
                raise AssertionError("discovery must not invoke author equality")

        first = self.make_mobject(7, UnhashableMobject)
        second = self.make_mobject(8, UnhashableMobject)
        callback = lambda obj: None
        updaters.add_updater(first, callback)
        updaters.add_updater(second, callback)
        updaters.add_updater(first, callback)
        context = RecordingContext()
        scene = SimpleNamespace(time=0.0, _canonical_authoring_context=context)
        first._scene = second._scene = scene
        session_id = updaters.prepare_canonical_callbacks(scene, context)
        self.addCleanup(updaters.release_session, session_id)
        self.assertEqual([call[1].semanticSlot for call in context.calls], [7, 7, 8])
        self.assertEqual(updaters.get_updaters(first), [callback, callback])

    def test_active_session_owns_detached_target_until_its_release(self):
        value = self.make_mobject()
        context = RecordingContext()
        scene = SimpleNamespace(time=0.0, _canonical_authoring_context=context)
        value._scene = scene
        updaters.add_updater(value, lambda obj: None)
        session_id = scene._noon_canonical_callback_session.session_id
        self.addCleanup(updaters.release_session, session_id)
        ref = weakref.ref(value)
        value._scene = None
        del value, context, scene
        gc.collect()
        self.assertIsNotNone(ref(), "an active session must retain its target")
        updaters.release_session(session_id)
        gc.collect()
        self.assertIsNone(ref(), "released targets must not remain rooted by discovery")


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
