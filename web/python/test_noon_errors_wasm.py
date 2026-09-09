"""Actual Rust/WASM -> JS -> Pyodide tests, run by typed-authoring-errors-smoke.mjs.

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
    NoonError, NoonForeignHandleError, NoonOwnershipError, NoonPendingError, NoonCallbackError,
    NoonStaleHandleError, NoonStalePublicationError, NoonValueError, NoonUnsupportedError,
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

    def test_actual_continuation_drive_retains_pending_and_terminal_callback_categories(self):
        store = wasm.WasmAuthoringStore.new()
        context = store.createSceneContext()
        target = circle(store)
        context.bindMobject("0", target)
        context.addUpdater(target, "1", 0)
        context.beginOrdinaryWait(0.25)
        player = context.createExecutionPlayer(0.25, 41)
        phase = player.initialCallbackPhaseJson()
        self.assertIsNotNone(phase)

        def state():
            return (player.debugFrameJson(), bytes(player.resourceBundleBytes()),
                    player.callbackTerminationJson(), context.liveExecutionOwnership())

        # All four actual worker-drive entrypoints retain the very same guard.
        calls = [(player.driveLiveSegmentToAuthoredTime, 0.25),
                 (player.driveLiveSegmentFromWallTime, 1000),
                 (player.reanchorLiveSegmentWake, 1000),
                 (player.liveSegmentWake, 1000)]
        before = state()
        for function, value in calls:
            with self.assertRaises(NoonPendingError) as caught:
                engine_call(function, value, operation="continuation.drive")
            self.assert_diagnostic(caught.exception, "pending_work")
            self.assertEqual(caught.exception.code, "publication.callback_pending")
            self.assertEqual(state(), before)
        player.failCallbackPhaseJson(phase)
        terminal = state()
        for function, value in calls:
            with self.assertRaises(NoonCallbackError) as caught:
                engine_call(function, value, operation="continuation.drive")
            self.assert_diagnostic(caught.exception, "callback_failure")
            self.assertEqual(caught.exception.code, "completion.callback_terminated")
            self.assertEqual(state(), terminal)
        # Endpoint-before-termination completion precedence is shared Rust policy.
        with self.assertRaises(NoonPendingError) as caught:
            engine_call(player.completeLiveSegment)
        self.assertEqual(codes(caught.exception)[-1], "completion.not_at_boundary")
        self.assertEqual(state(), terminal)
        context.returnExecutionPlayer(player)
        returned = snapshot(context)
        with self.assertRaises(NoonCallbackError) as caught:
            engine_call(context.liveWait, 0.1)
        self.assertEqual(caught.exception.category, "callback_failure")
        self.assertEqual(snapshot(context), returned)

    def test_public_live_advance_projection_preserves_coercion_and_time_rules(self):
        host.resetStore()
        from noon import Circle, Scene
        from _manim_scene import _context
        scene = Scene()
        target = Circle(radius=0.5)
        scene.add(target)
        live = scene.live_execution(1)
        context = _context(scene)
        live.wait(0.25)
        for name in ("advance_to", "evaluate"):
            method = getattr(live, name)
            before = snapshot(context)
            for value in (float("nan"), float("inf"), -float("inf")):
                with self.assertRaises(NoonValueError) as caught:
                    method(value)
                self.assert_diagnostic(caught.exception, "invalid_input")
                self.assertEqual(caught.exception.operation, "LiveExecution." + name)
                self.assertEqual(codes(caught.exception),
                    ["advance.evaluation", "evaluation.invalid_time"] if name == "advance_to"
                    else ["clock.invalid_scene_time"])
                self.assertEqual(snapshot(context), before)
            original = ValueError("pending stale invalid scene time")
            class NotATime:
                def __float__(self):
                    raise original
            with self.assertRaises(ValueError) as caught:
                method(NotATime())
            self.assertIs(caught.exception, original)
            self.assertEqual(snapshot(context), before)
        live.advance_to(-1)
        self.assertEqual(snapshot(context)[1]["time"], 0)
        live.advance_to(0.125)
        live.advance_to(0.0625)
        self.assertEqual(snapshot(context)[1]["time"], 0.125)
        live.evaluate(0.0625)
        self.assertEqual(snapshot(context)[1]["time"], 0.0625)
        live.advance_to(9)
        live.complete()
        self.assertEqual(snapshot(context)[1]["time"], 0.25)

    def test_public_live_transform_errors_preserve_python_coercion_and_recover(self):
        host.resetStore()
        from noon import Circle, Scene
        from _manim_scene import _context
        from _noon_errors import NoonError
        scene = Scene()
        target = Circle(radius=0.5)
        scene.add(target)
        live = scene.live_execution(1)
        context = _context(scene)
        operations = [
            ("set_translation", (2.0, -1.0), "translation", {"x": 2.0, "y": -1.0}),
            ("shift", (1.0, 2.0), "translation", {"x": 3.0, "y": 1.0}),
            ("set_scale", (2.0, 0.5), "scale", {"x": 2.0, "y": 0.5}),
            ("set_rotation", (0.5,), "rotation", 0.5),
        ]
        live.wait(0.25)
        for name, valid, field, expected in operations:
            with self.subTest(operation=name):
                method = getattr(live, name)
                before = (snapshot(context), target._semantic_handle.snapshotJson())
                # All argument coercion stays outside the shared engine call.
                sentinel = ValueError("foreign stale pending unsupported")
                class BadFloat:
                    def __float__(self):
                        raise sentinel
                with self.assertRaises(ValueError) as caught:
                    method(target, BadFloat(), *valid[1:])
                self.assertIs(caught.exception, sentinel)
                self.assertNotIsInstance(caught.exception, NoonError)
                self.assertEqual((snapshot(context), target._semantic_handle.snapshotJson()), before)
                with self.assertRaises(NoonValueError) as caught:
                    method(target, float("nan"), *valid[1:])
                error = caught.exception
                self.assert_diagnostic(error, "invalid_input")
                self.assertEqual(error.operation, "LiveExecution." + name)
                self.assertEqual(codes(error), ["live.publication", "publication.semantic",
                                               "transaction.non_finite_property_value"])
                self.assertEqual((snapshot(context), target._semantic_handle.snapshotJson()), before)
                method(target, *valid)
                authored = json.loads(target._semantic_handle.snapshotJson())
                effective = snapshot(context)[1]
                self.assertEqual(authored["transform"][field], expected)
                self.assertEqual(effective["objects"][0]["transform"][field], expected)
                self.assertEqual(effective["time"], 0)
        live.advance_to(0.25)
        live.complete()
        self.assertEqual(snapshot(context)[1]["time"], 0.25)

    def test_public_live_content_and_effective_query_errors_recover(self):
        host.resetStore()
        from noon import Circle, Square, Scene
        from _manim_scene import _context
        scene = Scene()
        target, source = Circle(radius=0.5), Square(side_length=0.75)
        scene.add(target)
        live = scene.live_execution(1)
        context = _context(scene)
        foreign_store = wasm.WasmAuthoringStore.new()
        foreign = circle(foreign_store)

        def state():
            return (snapshot(context), target._semantic_handle.snapshotJson(),
                    source._semantic_handle.snapshotJson())

        # Retain the ordinary Python wrapper; substitute only the real Rust
        # handle to exercise provenance at the actual language boundary.
        for wrapper, invoke, operation in [
            (target, lambda: live.replace_content(target, source), "replace_content"),
            (source, lambda: live.replace_content(target, source), "replace_content"),
            (target, lambda: live.effective_center(target), "effective_center"),
        ]:
            original = wrapper._semantic_handle
            before = state()
            wrapper._semantic_handle = foreign
            try:
                with self.assertRaises(NoonForeignHandleError) as caught:
                    invoke()
                self.assert_diagnostic(caught.exception, "foreign_handle")
                self.assertEqual(caught.exception.operation, "LiveExecution." + operation)
            finally:
                wrapper._semantic_handle = original
            self.assertEqual(state(), before)

        live.remove(target)
        before = state()
        with self.assertRaises(NoonStaleHandleError) as caught:
            live.effective_center(target)
        self.assertEqual(codes(caught.exception), ["live.publication", "publication.unknown_object"])
        self.assertEqual(state(), before)
        live.add(target)
        self.assertEqual(live.effective_center(target).x, 0)

        context.returnExecutionPlayer(context.createExecutionPlayer(1, 41))
        target._semantic_handle.shift(2, -1)
        before = state()
        for invoke, operation in [
            (lambda: live.replace_content(target, source), "replace_content"),
            (lambda: live.effective_center(target), "effective_center"),
        ]:
            with self.assertRaises(NoonStalePublicationError) as caught:
                invoke()
            self.assert_diagnostic(caught.exception, "stale_publication")
            self.assertEqual(caught.exception.operation, "LiveExecution." + operation)
            self.assertEqual(codes(caught.exception), ["live.publication", "publication.stale_scene_revision"])
            self.assertEqual(state(), before)
        # The explicit existing new-run boundary, not exception mapping, owns
        # replacement of the stale returned presentation runtime.
        context.prepareExecutionRun()
        context.beginLiveExecution(1)
        prior = json.loads(target._semantic_handle.snapshotJson())
        live.wait(0.25)
        live.replace_content(target, source)
        after = json.loads(target._semantic_handle.snapshotJson())
        self.assertEqual(after["geometry"], json.loads(source._semantic_handle.snapshotJson())["geometry"])
        self.assertEqual(after["transform"], prior["transform"])
        self.assertEqual(after["style"], prior["style"])
        center = live.effective_center(target)
        self.assertEqual((center.x, center.y), (2, -1))
        live.advance_to(0.25)
        live.complete()
        self.assertEqual(snapshot(context)[1]["time"], 0.25)

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

    def test_invalid_first_wait_does_not_publish_a_player_and_retries(self):
        # Existing-player failures are covered separately; these are fresh contexts.
        for method in ("ordinaryWait", "beginOrdinaryWait"):
            for populated in (False, True):
                for duration in (float("nan"), -1.0, float("-inf"), float("inf")):
                    with self.subTest(method=method, populated=populated, duration=duration):
                        store = wasm.WasmAuthoringStore.new()
                        context = store.createSceneContext()
                        target = circle(store) if populated else None
                        if target is not None:
                            context.bindMobject("0", target)

                        def state():
                            return (context.liveExecutionOwnership(), context.authoredDuration(),
                                    list(context.rootMembershipKeys()), context.liveHandoffDuration())

                        before = state()
                        expected = NoonError if duration == float("inf") else NoonValueError
                        with self.assertRaises(expected) as caught:
                            engine_call(getattr(context, method), duration, operation="Scene.wait")
                        self.assert_diagnostic(caught.exception, "unclassified" if duration == float("inf") else "invalid_input")
                        if duration != float("inf"):
                            self.assertEqual(codes(caught.exception)[-1], "segment.invalid_duration")
                        self.assertEqual(state(), before)
                        self.assertEqual(context.liveExecutionOwnership(), "none")
                        # Reuse the authored context and its binding reservations.
                        added = circle(store)
                        engine_call(context.bindMobject, "1", added)
                        endpoint = engine_call(context.beginOrdinaryWait, 0.5)
                        self.assertEqual(endpoint, 0.5)
                        player = context.createExecutionPlayer(0.5, 73)
                        drive = player.driveLiveSegmentToAuthoredTime(endpoint)
                        self.assertTrue(drive.reachedEndpoint)
                        drive.free()
                        engine_call(player.completeLiveSegment)
                        context.returnExecutionPlayer(player)
                        self.assertEqual(engine_call(context.ordinaryWait, 0.25), 0.75)
                        self.assertEqual(json.loads(context.liveDebugFrameJson())["time"], 0.75)
                        self.assertTrue(context.containsMobject(added))
                        context.free()
                        added.free()
                        if target is not None:
                            target.free()
                        store.free()

    def test_callback_family_shift_is_unique_and_atomic_with_same_phase_retry(self):
        import copy
        import _manim_updaters as updaters
        from noon import Circle, Group, Scene
        from _manim_scene import _context
        host.resetStore()
        scene=Scene()
        first, second, missing, untouched=Circle(),Circle(),Circle(),Circle()
        nested=Group(first,second)
        family=Group(first,nested)
        invalid=Group(first,missing)
        scene.add(first,second,untouched)
        context=_context(scene)
        context.addUpdater(first._semantic_handle,"73",0.0)
        context.beginOrdinaryWait(0.25)
        player=context.createExecutionPlayer(0.25,41)
        phase=json.loads(player.initialCallbackPhaseJson())
        player.initialDeltaJson()
        overlay=updaters._CanonicalCallbackContext(phase,context)
        reads=[]
        def pinned_read(kind,key):
            reads.append(kind)
            return json.loads(str(engine_call(player.requiredCallbackReadJson,
                json.dumps(phase["token"]),json.dumps({"kind":kind,"node":{"slot":key[0],"generation":key[1]}}))))
        overlay._read=pinned_read
        token=updaters._ACTIVE_CANONICAL_CONTEXT.set(overlay)
        updaters._ACTIVE_CONTEXTS[id(scene)]=overlay
        def observed():
            return (copy.deepcopy(overlay._rows),copy.deepcopy(overlay.effective_batch()),
                    player.debugFrameJson(),bytes(player.resourceBundleBytes()),
                    first._semantic_handle.snapshotJson(),context.liveExecutionOwnership())
        try:
            # The callback reads actual Rust-generated phase rows, not a forged
            # snapshot; only transport delivery is inline in this boundary test.
            first.shift((0.25,0))
            before_count=len(overlay.effective_batch()["writes"])
            self.assertIs(family.shift((1,0)),family)
            # Centers come from f32 effective bounds; rejection snapshots and
            # exact write counts below remain bit-for-bit checks.
            with self.subTest(reproduction="nested alias requested +1"):
                self.assertAlmostEqual(first.get_center().x, 1.25, delta=1e-6, msg="alias received a second translation")
                self.assertAlmostEqual(second.get_center().x, 1.0, delta=1e-6)
                self.assertEqual(len(overlay.effective_batch()["writes"])-before_count,2)
            family.shift((-1,0))
            before=observed()
            missing_handle=missing._semantic_handle
            missing._semantic_handle=None
            try:
                with self.subTest(reproduction="caught late member failure"):
                    with self.assertRaises((RuntimeError,ReferenceError)):
                        invalid.shift((1,0))
                    self.assertAlmostEqual(first.get_center().x, 0.25, delta=1e-6, msg="late failure retained an earlier +1 write")
                    self.assertEqual(observed(),before)
            finally:
                missing._semantic_handle=missing_handle
            with self.assertRaises(ValueError):
                family.shift((float("nan"),0))
            self.assertEqual(observed(),before)
            family.shift((1,0))
            successful=observed()
            with self.assertRaises((RuntimeError,ReferenceError)):
                invalid.shift((1,0))
            self.assertEqual(observed(),successful)
            family.shift((-1,0))
            self.assertAlmostEqual(first.get_center().x, 0.25, delta=1e-6)
            self.assertAlmostEqual(second.get_center().x, 0.0, delta=1e-6)
            self.assertIsNone(player.drainDeltaJson())
        finally:
            updaters._ACTIVE_CONTEXTS.pop(id(scene),None)
            updaters._ACTIVE_CANONICAL_CONTEXT.reset(token)
        resources=bytes(player.resourceBundleBytes())
        authored=first._semantic_handle.snapshotJson()
        engine_call(player.commitCallbackPhaseJson,json.dumps(overlay.effective_batch()))
        frame=json.loads(player.debugFrameJson())
        self.assertEqual(frame["objects"][0]["transform"]["translation"]["x"],0.25)
        self.assertEqual(frame["objects"][2]["transform"]["translation"]["x"],0.0)
        self.assertEqual(first._semantic_handle.snapshotJson(),authored)
        self.assertEqual(bytes(player.resourceBundleBytes()),resources)
        # Finish the same callback segment, acknowledge its actual endpoint,
        # and return the very same player without replaying a callback.
        for _ in range(3):
            drive=player.driveLiveSegmentToAuthoredTime(0.25)
            reached=drive.reachedEndpoint
            next_phase=drive.callbackPhaseJson
            drive.free()
            if reached: break
            self.assertIsNotNone(next_phase)
            next_phase=json.loads(next_phase)
            engine_call(player.commitCallbackPhaseJson,json.dumps({"token":next_phase["token"],"writes":[]}))
        self.assertTrue(reached)
        engine_call(player.completeLiveSegment)
        context.returnExecutionPlayer(player)
        self.assertEqual(context.liveExecutionOwnership(),"returned")
        self.assertEqual(json.loads(context.liveDebugFrameJson())["time"],0.25)


    def test_accepted_direct_handle_errors_preserve_paint_and_recover(self):
        store = wasm.WasmAuthoringStore.new()
        with self.assertRaises(NoonValueError) as caught:
            engine_call(store.createManimGeometry,
                        wasm.WasmManimGeometryOptions.circle(float("nan")))
        self.assert_diagnostic(caught.exception, "invalid_input")
        self.assertIn("authoring.invalid_render_number", codes(caught.exception))
        target = circle(store)
        before = target.snapshotJson()
        with self.assertRaises(NoonValueError) as caught:
            engine_call(target.setOpacity, 1.5)
        self.assertEqual(caught.exception.code, "authoring.invalid_opacity")
        self.assertEqual(target.snapshotJson(), before)
        engine_call(target.setOpacity, 0.5)
        self.assertAlmostEqual(target.fillOpacity, 0.5)
        self.assertAlmostEqual(target.strokeOpacity, 0.5)
        self.assertEqual(json.loads(target.snapshotJson())["style"]["opacity"], 1.0)

    def test_public_line_match_retains_unsupported_cause_and_recovers(self):
        host.resetStore()
        from noon import Line
        source = Line((-1, 0), (1, 0))
        target = Line((-1, 0), (1, 0)).scale((2, 1))
        before = source._semantic_handle.snapshotJson()
        with self.assertRaises(NoonUnsupportedError) as caught:
            source.match_points(target)
        self.assert_diagnostic(caught.exception, "unsupported_operation")
        self.assertEqual(caught.exception.operation, "Line.match_points")
        self.assertEqual(codes(caught.exception),
                         ["authoring.unsupported", "authoring.unsupported_operation"])
        self.assertEqual(source._semantic_handle.snapshotJson(), before)
        source.match_points(Line((0, 0), (0, 2)))
        self.assertAlmostEqual(source.get_end()[1], 2.0)

    def test_public_family_and_property_rejections_preserve_shared_state(self):
        host.resetStore()
        from noon import Circle, Square, VGroup
        first, second = Circle(), Square()
        group = VGroup(first, second)
        def state():
            return (first._semantic_handle.snapshotJson(), second._semantic_handle.snapshotJson())
        before = state()
        with self.assertRaises(NoonValueError) as caught:
            group.arrange_in_grid(rows=1, cols=1)
        self.assertEqual(caught.exception.code, "authoring.insufficient_grid_capacity")
        self.assertEqual(state(), before)
        with self.assertRaises(NoonValueError) as caught:
            first.shift((1e40, 0))
        self.assertEqual(caught.exception.code, "authoring.invalid_render_number")
        self.assertEqual(state(), before)
        group.arrange_in_grid(rows=1, cols=2)
        first.shift((0.5, 0))
        self.assertNotEqual(state(), before)

    def test_public_options_and_value_errors_use_the_existing_mapper(self):
        host.resetStore()
        from noon import Scene
        from _manim_animation_options import resolve
        from _manim_scene import _context
        def options(run_time):
            return resolve(builder_args={"run_time": run_time}, default_lag_ratio=0,
                           play_run_time=None, play_easing=None, play_rate_func=None,
                           play_lag_ratio=None)
        with self.assertRaises(NoonValueError) as caught:
            options(-1)
        self.assert_diagnostic(caught.exception, "invalid_input")
        self.assertEqual(caught.exception.operation, "animation.options")
        self.assertEqual(options(0.5).run_time, 0.5)
        scene = Scene()
        context = _context(scene)
        before = (list(context.rootMembershipKeys()), context.authoredDuration())
        with self.assertRaises(NoonValueError) as caught:
            scene.value_tracker(float("nan"))
        self.assertIn("signal.non_finite_value", codes(caught.exception))
        self.assertEqual((list(context.rootMembershipKeys()), context.authoredDuration()), before)
        tracker = scene.value_tracker(2.0)
        tracker.set_value(3.0)
        self.assertEqual(tracker.get_value(), 3.0)


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