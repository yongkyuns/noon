"""Real callback-transaction errors; loaded by the existing typed-error runner.

Native discovery explicitly skips these tests. The fixtures never forge semantic
handles or import diagnostic state into the engine; detached objects and callback
receipts come from actual public WASM operations.
"""
import copy
import json
import unittest

try:
    import _noon_wasm_for_error_tests as wasm
    import _noon_error_test_host as host
except ImportError:
    wasm = None

from _noon_errors import (
    NoonError, NoonCallbackError, NoonStaleHandleError, NoonStalePublicationError, NoonValueError,
    engine_call,
)


class CallbackFixture:
    def __init__(self, with_tracker=False, with_families=False):
        self.store = wasm.WasmAuthoringStore.new()
        self.context = self.store.createSceneContext()
        self.target = self.store.createManimCircle(0.5)
        self.detached = self.store.createManimCircle(0.2)
        self.families = []
        if with_families:
            for handles in ((self.target, self.detached), (self.target,)):
                members = wasm.WasmSceneMembershipBatch.new("add")
                for handle in handles:
                    members.appendMobject("", handle)
                self.families.append(self.store.createFamily(members, 0))
        self.context.bindMobject("0", self.target)
        self.context.addUpdater(self.target, "1", 0)
        self.tracker = self.context.createValueTracker(2.0) if with_tracker else None
        self.context.beginOrdinaryWait(0.25)
        self.player = self.context.createExecutionPlayer(0.25, 41)
        self.phase = json.loads(self.player.initialCallbackPhaseJson())
        self.player.initialDeltaJson()

    def close(self):
        if self.player is not None:
            self.player.free()
        self.context.free()
        self.target.free()
        self.detached.free()
        if self.tracker is not None:
            self.tracker.free()
        for family in self.families:
            family.free()
        self.store.free()

    def state(self):
        return (self.player.debugFrameJson(), bytes(self.player.resourceBundleBytes()),
                self.player.callbackTerminationJson(), self.context.liveExecutionOwnership())

    def commit(self, writes=None):
        engine_call(self.player.commitCallbackPhaseJson, json.dumps({
            "token": self.phase["token"], "writes": writes or [],
        }), operation="callback.commit")
        self.phase = None

    def advance(self):
        drive = self.player.driveLiveSegmentToAuthoredTime(0.25)
        try:
            if drive.callbackPhaseJson is not None:
                self.phase = json.loads(drive.callbackPhaseJson)
            return drive.reachedEndpoint
        finally:
            drive.free()

    def return_player(self):
        self.context.returnExecutionPlayer(self.player)
        self.player = None

    def finish(self):
        resources = bytes(self.player.resourceBundleBytes())
        if self.phase is not None:
            self.commit()
        if not self.advance():
            self.commit()
            assert self.advance()
        self.player.completeLiveSegment()
        assert bytes(self.player.resourceBundleBytes()) == resources
        assert json.loads(self.player.initialDeltaJson())["session"] == 41
        self.return_player()
        members = wasm.WasmSceneMembershipBatch.new("add")
        members.reserveMobjectBinding("1", self.detached)
        members.appendMobject("1", self.detached)
        self.context.editMembership(members)
        assert json.loads(self.context.liveDebugFrameJson())["time"] == 0.25
        assert self.context.liveExecutionOwnership() == "returned"


@unittest.skipIf(wasm is None, "requires the real browser WASM/Pyodide boundary")
class CallbackErrorBoundaryTests(unittest.TestCase):
    def fixture(self):
        fixture = CallbackFixture()
        self.addCleanup(fixture.close)
        return fixture

    def rejection(self, fixture, function, args, exception_type, category, code):
        before = fixture.state()
        with self.assertRaises(exception_type) as caught:
            engine_call(function, *args, operation="callback.transaction")
        error = caught.exception
        self.assertEqual(error.category, category)
        self.assertEqual(error.code, code)
        self.assertEqual(error.operation, "callback.transaction")
        self.assertEqual(str(error), str(error.js_error.message))
        self.assertTrue(str(error))
        self.assertTrue(host.isError(error.js_error))
        self.assertTrue(host.same(error.js_error, error.__cause__.js_error))
        self.assertEqual(fixture.state(), before)
        self.assertIsNone(fixture.player.drainDeltaJson())
        return error

    def test_foreign_abort_receipts_reject_even_with_matching_sequence_numbers(self):
        for method in ("failCallbackPhaseJson", "interruptCallbackPhaseJson"):
            with self.subTest(method=method):
                fixture, foreign = self.fixture(), self.fixture()
                self.assertEqual(fixture.phase["token"]["sequence"], foreign.phase["token"]["sequence"])
                self.assertNotEqual(fixture.phase["token"]["runtime"], foreign.phase["token"]["runtime"])
                self.rejection(fixture, getattr(fixture.player, method), [json.dumps(foreign.phase)],
                               NoonStalePublicationError, "stale_publication", "callback.stale_token")
                fixture.finish()

    def test_replayed_and_completed_abort_receipts_preserve_recovery(self):
        for method in ("failCallbackPhaseJson", "interruptCallbackPhaseJson"):
            for replay in (False, True):
                with self.subTest(method=method, replay=replay):
                    fixture = self.fixture()
                    old_phase = json.dumps(fixture.phase)
                    fixture.commit()
                    fixture.player.drainDeltaJson()
                    if replay:
                        self.assertFalse(fixture.advance())
                    code = "callback.stale_token" if replay else "callback.no_pending_phase"
                    self.rejection(fixture, getattr(fixture.player, method), [old_phase],
                                   NoonStalePublicationError, "stale_publication", code)
                    fixture.finish()

    def test_unknown_callback_reads_preserve_the_pending_effective_read_view(self):
        fixture = self.fixture()
        node = {"slot": fixture.detached.semanticSlot, "generation": fixture.detached.semanticGeneration}
        for kind, code in (("object", "callback_read.unknown_object"),
                           ("scalar_signal", "callback_read.unknown_signal")):
            with self.subTest(kind=kind):
                self.rejection(fixture, fixture.player.requiredCallbackReadJson,
                               [json.dumps(fixture.phase["token"]), json.dumps({"kind": kind, "node": node})],
                               NoonStaleHandleError, "stale_handle", code)
        observed = json.loads(engine_call(fixture.player.requiredCallbackReadJson,
                              json.dumps(fixture.phase["token"]), json.dumps({
                                  "kind": "object", "node": fixture.phase["objects"][0]["node"],
                              }), operation="callback.read"))
        self.assertEqual(observed["object"]["transform"], fixture.phase["objects"][0]["transform"])
        fixture.finish()

    def test_batch_rejection_does_not_publish_an_earlier_valid_write(self):
        fixture = self.fixture()
        transform = copy.deepcopy(fixture.phase["objects"][0]["transform"])
        transform["translation"]["x"] = 3.0
        valid = {"kind": "transform", "object": fixture.phase["objects"][0]["node"], "transform": transform}
        invalid = {"kind": "transform", "object": {
            "slot": fixture.detached.semanticSlot, "generation": fixture.detached.semanticGeneration,
        }, "transform": transform}
        self.rejection(fixture, fixture.player.commitCallbackPhaseJson,
                       [json.dumps({"token": fixture.phase["token"], "writes": [valid, invalid]})],
                       NoonStaleHandleError, "stale_handle", "callback.unknown_object")
        before = fixture.player.debugFrameJson()
        fixture.commit([valid])
        self.assertNotEqual(fixture.player.debugFrameJson(), before)
        self.assertIsNotNone(fixture.player.drainDeltaJson())
        fixture.finish()

    def test_family_read_preserves_nested_cause_and_same_player_recovery(self):
        fixture = CallbackFixture(with_families=True)
        self.addCleanup(fixture.close)
        token = json.dumps(fixture.phase["token"])
        def request(handle):
            return json.dumps({"kind": "family", "node": {
                "slot": handle.semanticSlot, "generation": handle.semanticGeneration,
            }})
        error = self.rejection(fixture, fixture.player.requiredCallbackReadJson,
                               [token, request(fixture.families[0])],
                               NoonStaleHandleError, "stale_handle", "callback.family.read")
        self.assertEqual(error.rust_cause.code, "callback.read")
        self.assertEqual(error.rust_cause.cause.code, "callback_read.unknown_object")
        self.assertIsNone(error.rust_cause.cause.cause)
        self.assertEqual(error.rust_cause.category, "stale_handle")
        self.assertEqual(error.rust_cause.cause.category, "stale_handle")
        before = fixture.state()
        value = json.loads(engine_call(fixture.player.requiredCallbackReadJson,
                           token, request(fixture.families[1]), operation="callback.read"))
        self.assertEqual(value["kind"], "family")
        self.assertEqual(value["objects"], fixture.phase["objects"])
        self.assertEqual(fixture.state(), before)
        self.assertIsNone(fixture.player.drainDeltaJson())
        fixture.finish()

    def test_family_paint_retains_typed_causes_and_same_player_recovery(self):
        cases = (
            ("Opacity", None, 1.5, "authoring.invalid_opacity"),
            ("Opacity", None, float("nan"), "authoring.invalid_render_number"),
            ("Stroke", -1.0, None, "authoring.negative_stroke_width"),
            ("Stroke", float("inf"), None, "authoring.invalid_render_number"),
        )
        for operation, width, opacity, cause_code in cases:
            with self.subTest(operation=operation, cause=cause_code):
                fixture = CallbackFixture(with_families=True)
                self.addCleanup(fixture.close)
                family = fixture.families[1]
                revision = fixture.phase["token"]["publication"]["scene_revision"]
                authored = (fixture.target.snapshotJson(), fixture.detached.snapshotJson())
                # An empty read cache proves input rejection precedes effective reads.
                error = self.rejection(
                    fixture, fixture.context.callbackFamilyPaint,
                    [family, revision, operation, "[]", False, 0.0, 0.0, 0.0, 1.0,
                     width, opacity],
                    NoonValueError, "invalid_input", "callback.family.invalid_paint",
                )
                self.assertIsNotNone(error.rust_cause)
                self.assertEqual(error.rust_cause.category, "invalid_input")
                self.assertEqual(error.rust_cause.code, cause_code)
                self.assertIsNone(error.rust_cause.cause)
                self.assertTrue(host.isError(error.js_error.cause))
                self.assertEqual(str(error.js_error.cause.code), cause_code)
                self.assertTrue(error.rust_cause.message)
                self.assertEqual((fixture.target.snapshotJson(), fixture.detached.snapshotJson()),
                                 authored)

                rows = [[row["node"]["slot"], row["node"]["generation"], row["style"]]
                        for row in fixture.phase["objects"]]
                before = fixture.state()
                changes = json.loads(engine_call(
                    fixture.context.callbackFamilyPaint, family, revision, operation,
                    json.dumps(rows), False, 0.0, 0.0, 0.0, 1.0,
                    0.1 if operation == "Stroke" else None, 0.5,
                    operation="callback.family.paint",
                ))
                self.assertEqual(len(changes), 1)
                self.assertEqual(fixture.state(), before)
                self.assertEqual((fixture.target.snapshotJson(), fixture.detached.snapshotJson()),
                                 authored)
                fixture.commit([{
                    "kind": "style", "object": {"slot": slot, "generation": generation},
                    "style": style,
                } for slot, generation, style in changes])
                self.assertNotEqual(fixture.state(), before)
                self.assertEqual((fixture.target.snapshotJson(), fixture.detached.snapshotJson()),
                                 authored)
                fixture.finish()

    def test_valid_abort_remains_terminal_and_repeat_keeps_no_pending_precedence(self):
        for method, kind in (("failCallbackPhaseJson", "failed"),
                             ("interruptCallbackPhaseJson", "interrupted")):
            with self.subTest(method=method):
                fixture = self.fixture()
                raw = json.dumps(fixture.phase)
                engine_call(getattr(fixture.player, method), raw, operation="callback.abort")
                self.assertEqual(json.loads(fixture.player.callbackTerminationJson())["kind"], kind)
                self.rejection(fixture, getattr(fixture.player, method), [raw],
                               NoonStalePublicationError, "stale_publication", "callback.no_pending_phase")
                terminal = fixture.state()
                with self.assertRaises(NoonCallbackError):
                    engine_call(fixture.player.driveLiveSegmentToAuthoredTime, 0.25)
                self.assertEqual(fixture.state(), terminal)
                fixture.return_player()
                before = fixture.context.liveDebugFrameJson()
                with self.assertRaises(NoonCallbackError):
                    engine_call(fixture.context.liveWait, 0.1)
                self.assertEqual(fixture.context.liveDebugFrameJson(), before)

    def test_decoder_and_local_guard_failures_stay_explicitly_unclassified(self):
        fixture, foreign = self.fixture(), self.fixture()
        for function, args in (
            (fixture.player.failCallbackPhaseJson, ["{broken json"]),
            (fixture.player.interruptCallbackPhaseJson, ["{broken json"]),
            (fixture.player.commitCallbackPhaseJson, ["{broken json"]),
            (fixture.player.requiredCallbackReadJson, ["{}", "{}"]),
            (fixture.player.commitCallbackPhaseJson, [json.dumps({"token": foreign.phase["token"], "writes": []})]),
            (fixture.player.requiredCallbackReadJson, [json.dumps(foreign.phase["token"]), "{}"]),
        ):
            with self.subTest(function=str(function)):
                self.rejection(fixture, function, args, NoonError, "unclassified", "unclassified")
        fixture.finish()


async def check_sparse_callback_read_callsite():
    """Run the existing Python context method against an actual WASM read Promise."""
    from _manim_updaters import _CanonicalCallbackContext

    fixture = CallbackFixture(with_tracker=True)
    restore = host.installCallbackReader(fixture.player)
    try:
        context = _CanonicalCallbackContext(fixture.phase, fixture.context)
        missing = (int(fixture.detached.semanticSlot), int(fixture.detached.semanticGeneration))
        before = fixture.state()
        try:
            await context._read_scalar_async(missing)
        except NoonStaleHandleError as error:
            assert error.category == "stale_handle" and error.code == "callback_read.unknown_signal"
            assert error.operation == "callback.read"
            assert host.isError(error.js_error)
            assert host.same(error.js_error, error.__cause__.js_error)
            # Prefetch stores an actual error until the scalar is really requested.
            # Exercise that ordinary propagation site without replaying a callback.
            context._prefetch_errors[missing] = error
            try:
                context.scalar(missing)
            except NoonStaleHandleError as deferred:
                assert deferred is error
            else:
                raise AssertionError("deferred typed read error was swallowed")
        else:
            raise AssertionError("unknown callback signal was accepted")
        assert fixture.state() == before
        assert fixture.player.drainDeltaJson() is None
        present = (int(fixture.tracker.semanticSlot), int(fixture.tracker.semanticGeneration))
        assert await context._read_scalar_async(present) == 2.0
        fixture.finish()
    finally:
        restore()
        fixture.close()
