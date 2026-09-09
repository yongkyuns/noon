"""Actual Rust/WASM -> JS -> Pyodide tests, run by authoring-errors-smoke.mjs.

The native discovery suite skips this module; it is not mocked WASM coverage.
Snapshots below use the existing explicit debug boundary only as test evidence.
"""
import json
import unittest

try:
    import _noon_wasm_for_error_tests as wasm
    import _noon_error_test_host as host
except ImportError:
    wasm = None

from _noon_errors import (
    NoonForeignHandleError, NoonOwnershipError, NoonPendingError,
    NoonStaleHandleError, NoonStalePublicationError, NoonValueError,
    engine_await, engine_call,
)


def circle(store):
    return store.createManimGeometry(wasm.WasmManimGeometryOptions.circle(1.0))


def codes(error):
    result = [error.code]
    cause = error.rust_cause
    while cause is not None:
        result.append(cause.code)
        cause = cause.cause
    return result


def edit(context, kind, *members, bindings=()):
    batch = context.beginMembershipBatch(kind)
    for wrapper, member in bindings:
        batch.reserveMobjectBinding(str(wrapper), member)
    for wrapper, member in members:
        batch.appendMobject(str(wrapper), member)
    return engine_call(context.editMembership, batch, operation="Scene." + kind)


def snapshot(context):
    return (list(context.rootMembershipKeys()), json.loads(context.liveDebugFrameJson()),
            context.liveHandoffDuration(), context.liveExecutionOwnership())


@unittest.skipIf(wasm is None, "requires the actual browser WASM/Pyodide fixture")
class WasmErrorProjectionTests(unittest.TestCase):
    def assert_diagnostic(self, error, category):
        self.assertEqual(error.category, category)
        self.assertEqual(error.js_error.noonErrorVersion, 1)
        self.assertEqual(str(error), error.js_error.message)
        self.assertTrue(str(error).strip())
        self.assertIsNotNone(error.__cause__)
        self.assertTrue(host.same(error.js_error, error.__cause__.js_error))

    def test_foreign_colliding_handle_and_recovery(self):
        first, second = wasm.WasmAuthoringStore.new(), wasm.WasmAuthoringStore.new()
        context, other = first.createSceneContext(), second.createSceneContext()
        local, foreign = circle(first), circle(second)
        self.assertEqual((local.semanticSlot, local.semanticGeneration),
                         (foreign.semanticSlot, foreign.semanticGeneration))
        context.beginLiveExecution(1)
        before = snapshot(context)
        with self.assertRaises(NoonForeignHandleError) as caught:
            engine_call(context.bindMobject, "0", foreign)
        self.assert_diagnostic(caught.exception, "foreign_handle")
        self.assertEqual(caught.exception.code, "authoring.foreign_store")
        self.assertEqual(snapshot(context), before)
        engine_call(context.bindMobject, "0", local)
        self.assertTrue(context.containsMobject(local))
        self.assertEqual(list(other.rootMembershipKeys()), [])

    def test_stale_generation_preserves_cause_without_freeing_js_wrapper(self):
        store = wasm.WasmAuthoringStore.new()
        context = store.createSceneContext()
        stale = wasm.authoringErrorStaleMobjectSmoke(store)
        replacement = circle(store)
        context.beginLiveExecution(1)
        before = snapshot(context)
        with self.assertRaises(NoonStaleHandleError) as caught:
            engine_call(context.containsMobject, stale)
        error = caught.exception
        self.assert_diagnostic(error, "stale_handle")
        self.assertEqual(codes(error), ["authoring.semantic", "semantic.unknown_node"])
        self.assertEqual(error.rust_cause.message,
                         f"unknown semantic node {stale.semanticSlot}:{stale.semanticGeneration}")
        self.assertEqual(snapshot(context), before)
        engine_call(context.bindMobject, "0", replacement)
        self.assertTrue(context.containsMobject(replacement))

    def test_membership_failure_preserves_frame_and_binding_reservations(self):
        store = wasm.WasmAuthoringStore.new()
        context = store.createSceneContext()
        present, missing, replacement = circle(store), circle(store), circle(store)
        context.bindMobject("0", present)
        context.beginLiveExecution(1)
        before = snapshot(context)
        with self.assertRaises(NoonValueError) as caught:
            edit(context, "replace", ("2", missing), ("1", replacement),
                 bindings=(("2", missing), ("1", replacement)))
        self.assert_diagnostic(caught.exception, "invalid_input")
        self.assertEqual(codes(caught.exception)[-1], "membership.missing_target")
        self.assertEqual(snapshot(context), before)
        # Reuse the same wrapper reservation after rejection: no partial bind escaped.
        edit(context, "replace", ("0", present), ("1", replacement),
             bindings=(("0", present), ("1", replacement)))
        self.assertFalse(context.containsMobject(present))
        self.assertTrue(context.containsMobject(replacement))

    def test_wait_input_and_pending_completion_then_recovery(self):
        store = wasm.WasmAuthoringStore.new()
        context = store.createSceneContext()
        added = circle(store)
        context.beginLiveExecution(1)
        before = snapshot(context)
        with self.assertRaises(NoonValueError) as caught:
            engine_call(context.liveWait, float("nan"))
        self.assert_diagnostic(caught.exception, "invalid_input")
        self.assertEqual(codes(caught.exception)[-1], "segment.invalid_duration")
        self.assertEqual(snapshot(context), before)
        context.liveWait(0.5)
        pending = snapshot(context)
        with self.assertRaises(NoonPendingError) as caught:
            engine_call(context.bindMobject, "0", added)
        self.assertEqual(caught.exception.code, "publication.segment_pending")
        self.assertEqual(snapshot(context), pending)
        with self.assertRaises(NoonPendingError) as caught:
            engine_call(context.liveCompleteSegment)
        self.assertEqual(codes(caught.exception)[-1], "completion.not_at_boundary")
        self.assertEqual(snapshot(context), pending)
        context.liveAdvanceSegmentTo(0.5)
        engine_call(context.liveCompleteSegment)
        engine_call(context.bindMobject, "0", added)
        self.assertTrue(context.containsMobject(added))

    def test_stale_publication_does_not_reconcile_authored_into_effective(self):
        store = wasm.WasmAuthoringStore.new()
        context = store.createSceneContext()
        present, added = circle(store), circle(store)
        context.bindMobject("0", present)
        context.beginLiveExecution(1)
        context.returnExecutionPlayer(context.createExecutionPlayer(1, 41))
        before = snapshot(context)
        present.shift(2, 0)
        with self.assertRaises(NoonStalePublicationError) as caught:
            engine_call(context.bindMobject, "1", added)
        self.assert_diagnostic(caught.exception, "stale_publication")
        self.assertEqual(codes(caught.exception)[-1], "publication.stale_scene_revision")
        self.assertEqual(snapshot(context), before)
        # The existing explicit returned-player/new-run boundary owns recovery.
        context.prepareExecutionRun()
        context.beginLiveExecution(1)
        engine_call(context.bindMobject, "1", added)
        self.assertTrue(context.containsMobject(added))
        self.assertNotEqual(snapshot(context)[1], before[1])

    def test_ownership_rejection_retains_exact_player_and_rightful_return(self):
        store, foreign_store = wasm.WasmAuthoringStore.new(), wasm.WasmAuthoringStore.new()
        contexts = [store.createSceneContext(), store.createSceneContext(),
                    foreign_store.createSceneContext()]
        players = [context.createExecutionPlayer(1, 41) for context in contexts]
        for index in (1, 2):
            before = json.loads(players[index].debugFrameJson())
            with self.assertRaises(NoonOwnershipError) as caught:
                engine_call(contexts[0].returnExecutionPlayer, players[index])
            error = caught.exception
            self.assert_diagnostic(error, "ownership")
            self.assertEqual(error.code, "ownership.foreign_scene")
            self.assertTrue(host.isError(error.js_error))
            self.assertEqual([c.liveExecutionOwnership() for c in contexts], ["transferred"] * 3)
            players[index] = error.take_player()
            self.assertEqual(json.loads(players[index].debugFrameJson()), before)
        self.assertEqual([json.loads(p.initialDeltaJson())["session"] for p in players], [41]*3)
        for context, player in zip(contexts, players):
            context.returnExecutionPlayer(player)
        self.assertEqual([c.liveExecutionOwnership() for c in contexts], ["returned"] * 3)

    def test_public_python_scene_batch_failure_is_atomic_and_recovers(self):
        host.resetStore()
        from noon import Circle, Scene, NoonForeignHandleError as PublicError
        from _manim_scene import _context
        scene = Scene()
        context = _context(scene)
        first, second = Circle(), Circle()
        correct = second._semantic_handle
        other = wasm.WasmAuthoringStore.new()
        # Allocate non-colliding wrapper keys; the foreign provenance is the error.
        circle(other)
        circle(other)
        foreign = circle(other)
        second._semantic_handle = foreign
        before = (scene._next_object_id, dict(scene._object_keys), list(scene.mobjects))
        with self.assertRaises(PublicError):
            scene.add(first, second)
        self.assertEqual((scene._next_object_id, dict(scene._object_keys), list(scene.mobjects)), before)
        self.assertIsNone(first._scene)
        self.assertIsNone(second._scene)
        second._semantic_handle = correct
        scene.add(first, second)
        self.assertEqual(scene.mobjects, [first, second])
        self.assertTrue(context.containsMobject(correct))


async def check_real_promise_rejection():
    """A real JS Promise rejects with a failure produced by actual WASM completion."""
    store = wasm.WasmAuthoringStore.new()
    context = store.createSceneContext()
    context.beginLiveExecution(1)
    context.liveWait(0.25)
    before = snapshot(context)
    try:
        await engine_await(host.completeAsPromise(context), operation="Scene.continuation")
    except NoonPendingError as error:
        assert codes(error)[-1] == "completion.not_at_boundary"
        assert error.operation == "Scene.continuation"
        assert host.isError(error.js_error)
        assert error.__cause__ is not None
    else:
        raise AssertionError("incomplete continuation unexpectedly succeeded")
    assert snapshot(context) == before
    context.liveAdvanceSegmentTo(0.25)
    await engine_await(host.completeAsPromise(context), operation="Scene.continuation")
    assert snapshot(context)[1]["time"] == 0.25
