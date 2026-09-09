#!/usr/bin/env node
// Real WASM/Pyodide boundary qualification for #1292 R3.
// Reuses the regression-owner fixtures from 862df403c57a9a98c24fec6ef83580c8e2e910e0;
// their provisional contract is aligned here to the single production mapper.
// Snapshots below are observations, never engine input or message classification.
import assert from "node:assert/strict";
import { mkdir, readFile, writeFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { chromium } from "playwright";
import { serveRepository } from "./browser-test-server.mjs";
import { PYTHON_COMPAT_MODULES } from "../web/python-compat-modules.js";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const artifacts = path.resolve(process.env.NOON_TYPED_ERRORS_ARTIFACTS ?? path.join(root, "browser-smoke-artifacts/typed-authoring-errors"));
const worker = await readFile(path.join(root, "web/python-worker.source.js"), "utf8");
const pyodideUrl = worker.match(/import \{ loadPyodide \} from "([^"]+)";/)?.[1];
assert.ok(pyodideUrl, "use the product worker's pinned Pyodide");
const modules = await Promise.all(PYTHON_COMPAT_MODULES.map(async ({sourcePath, runtimePath}) => ({
  runtimePath, source: await readFile(path.join(root, "web", sourcePath), "utf8"),
})));
const tests = await readFile(path.join(root, "web/python/test_noon_errors_wasm.py"), "utf8");
await mkdir(artifacts, {recursive: true});
const server = await serveRepository(root, Number(process.env.NOON_ERROR_TEST_PORT ?? 8798));
let browser;
const consoleMessages = [];
const report = {pyodideUrl, javascript: [], python: null, publicPython: null};
try {
  browser = await chromium.launch({headless: true});
  const page = await browser.newPage();
  page.setDefaultTimeout(180_000);
  page.on("console", message => { consoleMessages.push(message.text()); console.log(`[browser] ${message.text()}`); });
  page.on("pageerror", error => consoleMessages.push(error.stack ?? String(error)));
  await page.goto(`${server.baseUrl}/web/authoring-errors-smoke.html`);
  await page.evaluate(async () => {
    const wasm = await import("/web/pkg/noon_web.js");
    await wasm.default();
    const check = (value, message) => { if (!value) throw new Error(message); };
    const equal = (actual, expected, message) => check(JSON.stringify(actual) === JSON.stringify(expected), message);
    const key = handle => `${handle.semanticSlot}:${handle.semanticGeneration}`;
    const batch = (kind, pairs) => {
      const value = new wasm.WasmSceneMembershipBatch(kind);
      for (const [id, handle] of pairs) {
        value.reserveMobjectBinding(String(id), handle);
        value.appendMobject(String(id), handle);
      }
      return value;
    };
    const family = (store, ...handles) => {
      const members = new wasm.WasmSceneMembershipBatch("add");
      for (const handle of handles) members.appendMobject("", handle);
      return store.createFamily(members);
    };
    const snapshot = (context, live) => ({
      members: Array.from(context.rootMembershipKeys()), duration: context.authoredDuration(),
      handoff: context.liveHandoffDuration() ?? null, ownership: context.liveExecutionOwnership(),
      frame: live ? context.liveDebugFrameJson() : null,
    });
    const describe = error => ({category: error.category, code: error.code, message: error.message,
      cause: error.cause ? describe(error.cause) : null});
    const requireFailure = (invoke, category) => {
      let rejected;
      try { invoke(); } catch (error) { rejected = error; }
      check(rejected instanceof Error, "WASM must reject with an actual JavaScript Error");
      check(rejected.noonErrorVersion === 1, "missing shared projection version");
      check(rejected.category === category, `wrong category: ${rejected.category}`);
      check(typeof rejected.code === "string" && rejected.code.length > 0, "missing code");
      check(typeof rejected.message === "string" && rejected.message.length > 0, "missing diagnostic");
      return rejected;
    };
    const membershipFixture = (kind, live = false) => {
      const store = new wasm.WasmAuthoringStore(), otherStore = new wasm.WasmAuthoringStore();
      const context = store.createSceneContext(), otherContext = otherStore.createSceneContext();
      const first = store.createManimCircle(0.5), foreign = otherStore.createManimCircle(0.5);
      check(key(first) === key(foreign), "exercise equal numeric IDs in different stores");
      const next = store.createManimSquare(0.5);
      const recovery = [store.createManimCircle(0.2), store.createManimSquare(0.2)];
      const families = [];
      if (kind === "ambiguous" || kind === "cross_root") {
        const leaves = kind === "cross_root" ? [first, next] : [first];
        families.push(family(store, ...leaves), family(store, ...leaves));
        const initial = new wasm.WasmSceneMembershipBatch("add");
        initial.reserveMobjectBinding("0", first);
        if (kind === "cross_root") initial.reserveMobjectBinding("1", next);
        for (const value of families) initial.appendFamily(value);
        context.editMembership(initial);
      }
      if (kind === "pending") { context.beginOrdinaryWait(0.25); live = true; }
      else if (live) context.beginLiveExecution(1.0);
      const before = snapshot(context, live);
      return {
        reject: () => {
          if (kind === "cross_root") return context.editMembership(batch("remove", [[0, first]]));
          const replacing = kind === "missing" || kind === "ambiguous";
          context.editMembership(batch(replacing ? "replace" : "add", [[0, first], [1, kind === "foreign" ? foreign : next]]));
        },
        assertAtomic: () => equal(snapshot(context, live), before, `${kind}: rejection changed coherent state`),
        recover: () => {
          if (kind === "pending") {
            const player = context.createExecutionPlayer(0.25, 73);
            const drive = player.driveLiveSegmentToAuthoredTime(0.25);
            check(drive.reachedEndpoint, "continuation did not reach its endpoint");
            drive.free(); player.completeLiveSegment(); context.returnExecutionPlayer(player);
          }
          if (kind === "cross_root") {
            context.editMembership(new wasm.WasmSceneMembershipBatch("clear"));
            context.editMembership(batch("add", [[1, next]]));
            equal(Array.from(context.rootMembershipKeys()), [key(next)], "cross-root recovery failed");
          } else if (kind === "ambiguous") {
            context.editMembership(new wasm.WasmSceneMembershipBatch("clear"));
            // 0 was already bound; only rejected reservation 1 must be reusable.
            context.editMembership(batch("add", [[0, first], [1, recovery[1]]]));
            equal(Array.from(context.rootMembershipKeys()), [key(first), key(recovery[1])], "ambiguous recovery failed");
          } else {
            // Different valid nodes detect leaked reservations, not just equality.
            context.editMembership(batch("add", [[0, recovery[0]], [1, recovery[1]]]));
            equal(Array.from(context.rootMembershipKeys()), recovery.map(key), `${kind}: reservations leaked`);
          }
          if (kind === "pending") check(JSON.parse(context.liveDebugFrameJson()).time === 0.25, "recovery restarted the timeline");
          return snapshot(context, live);
        },
        dispose: () => {
          context.free(); otherContext.free();
          for (const value of [...families, first, foreign, next, ...recovery]) value.free();
          store.free(); otherStore.free();
        },
      };
    };
    const ownershipFixture = index => {
      const store = new wasm.WasmAuthoringStore(), foreignStore = new wasm.WasmAuthoringStore();
      const contexts = [store.createSceneContext(), store.createSceneContext(), foreignStore.createSceneContext()];
      const players = contexts.map(context => context.createExecutionPlayer(1, 41));
      const frames = players.map(player => player.debugFrameJson());
      const resources = players.map(player => Array.from(player.resourceBundleBytes()));
      const phases = () => contexts.map(context => context.liveExecutionOwnership());
      const before = phases();
      return {
        reject: () => contexts[0].returnExecutionPlayer(players[index]),
        assertAtomic: () => equal(phases(), before, "return rejection changed ownership"),
        recover: player => {
          players[index] = player;
          equal(players.map(value => value.debugFrameJson()), frames, "takePlayer changed the runtime");
          equal(players.map(value => Array.from(value.resourceBundleBytes())), resources, "takePlayer changed resources");
          equal(players.map(value => JSON.parse(value.initialDeltaJson()).session), [41, 41, 41], "takePlayer changed sessions");
          contexts.forEach((context, i) => context.returnExecutionPlayer(players[i]));
          equal(phases(), ["returned", "returned", "returned"], "rightful returns failed");
          return phases();
        },
        dispose: () => { for (const context of contexts) context.free(); store.free(); foreignStore.free(); },
      };
    };
    // The same real Rust operations run directly in JS and through Pyodide below.
    const livePropertyMethods = [
      {method: "liveSetTranslation", args: [2, -1], property: "Translation", field: "translation", expected: {x: 2, y: -1}},
      {method: "liveShift", args: [2, -1], property: "Translation", field: "translation", expected: {x: 2, y: -1}},
      {method: "liveSetScale", args: [2, 0.5], property: "Scale", field: "scale", expected: {x: 2, y: 0.5}},
      {method: "liveSetRotation", args: [0.5], property: "RotationZ", field: "rotation", expected: 0.5},
    ];
    const livePropertyCases = livePropertyMethods.flatMap(spec => [
      ...spec.args.flatMap((_, axis) => ["nan", "positive_infinity", "negative_infinity"].map(kind => ({...spec, kind, axis}))),
      ...["foreign", "stale"].map(kind => ({...spec, kind, axis: 0})),
    ]);
    const livePropertyFixture = spec => {
      const store = new wasm.WasmAuthoringStore(), otherStore = new wasm.WasmAuthoringStore();
      const context = store.createSceneContext();
      const object = store.createManimCircle(0.5), foreign = otherStore.createManimCircle(0.5);
      const stale = spec.kind === "stale" ? wasm.authoringErrorStaleMobjectSmoke(store) : null;
      context.bindMobject("0", object);
      context.beginLiveExecution(1);
      // Ordinary waits permit these property writes. Do not add a Python or
      // binding-side blanket "pending segment" rejection to make errors uniform.
      context.liveWait(0.25);
      const state = () => ({...snapshot(context, true), authored: object.snapshotJson()});
      const before = state();
      const args = [...spec.args];
      const invalid = {nan: NaN, positive_infinity: Infinity, negative_infinity: -Infinity};
      if (Object.hasOwn(invalid, spec.kind)) args[spec.axis] = invalid[spec.kind];
      const target = spec.kind === "foreign" ? foreign : spec.kind === "stale" ? stale : object;
      return {
        category: spec.kind === "foreign" ? "foreign_handle" : spec.kind === "stale" ? "stale_handle" : "invalid_input",
        reject: () => context[spec.method](target, ...args),
        assertDiagnostic: error => {
          if (Object.hasOwn(invalid, spec.kind)) {
            equal([error.code, error.cause?.code, error.cause?.cause?.code],
              ["live.publication", "publication.semantic", "transaction.non_finite_property_value"], "property cause chain was flattened");
            const leaf = error.cause.cause;
            check(leaf.cause === undefined, "leaf must not invent a source");
            equal(leaf.message, `semantic transaction mutation 0 cannot set property ${spec.property} on object ${key(object)} to a non-finite value`, "property diagnostic lost identity/context");
          } else if (spec.kind === "foreign") {
            check(error.code === "authoring.foreign_store", "foreign property handle misclassified");
          } else {
            equal([error.code, error.cause?.code], ["authoring.semantic", "semantic.unknown_node"], "stale property handle cause lost");
          }
        },
        assertAtomic: () => equal(state(), before, "rejected property changed authored/effective state, publication, roots or ownership"),
        recover: () => {
          context[spec.method](object, ...spec.args);
          const authored = JSON.parse(object.snapshotJson());
          const effective = JSON.parse(context.liveDebugFrameJson());
          equal(authored.transform[spec.field], spec.expected, "retry changed the wrong authored coordinate");
          equal(effective.objects[0].transform[spec.field], spec.expected, "retry was not coherently published");
          equal(effective.time, 0, "property write advanced the wait clock");
          const old = JSON.parse(before.frame).publication;
          equal(effective.publication.scene_revision, old.scene_revision + 1, "retry must commit one semantic revision");
          equal(effective.publication.execution_revision, old.execution_revision + 1, "retry must commit one execution revision");
          equal(effective.publication.frame_epoch, old.frame_epoch + 1, "retry must publish one frame epoch");
          context.liveAdvanceSegmentTo(0.25);
          context.liveCompleteSegment();
          equal(JSON.parse(context.liveDebugFrameJson()).time, 0.25, "retry lost the original continuation");
          return {atomic: true, propertyRetry: true, continuationCompleted: true};
        },
        dispose: () => { context.free(); object.free(); foreign.free(); stale?.free(); store.free(); otherStore.free(); },
      };
    };
    window.noonTypedErrorFixtures = {membershipFixture, ownershipFixture, livePropertyFixture, livePropertyCases, describe, requireFailure};
  });
  report.javascript = await page.evaluate(() => {
    const {membershipFixture, ownershipFixture, describe, requireFailure} = window.noonTypedErrorFixtures;
    const results = [];
    for (const [kind, live, category] of [
      ["foreign", false, "foreign_handle"], ["missing", false, "invalid_input"],
      ["ambiguous", false, "invalid_input"], ["foreign", true, "foreign_handle"],
      ["missing", true, "invalid_input"], ["pending", true, "pending_work"],
      ["cross_root", false, "unsupported_operation"], ["cross_root", true, "unsupported_operation"],
    ]) {
      const fixture = membershipFixture(kind, live);
      try {
        const error = requireFailure(fixture.reject, category);
        fixture.assertAtomic();
        results.push({kind, live, error: describe(error), recovered: fixture.recover()});
      } finally { fixture.dispose(); }
    }
    for (const index of [1, 2]) {
      const fixture = ownershipFixture(index);
      try {
        const error = requireFailure(fixture.reject, "ownership");
        fixture.assertAtomic();
        const diagnostic = describe(error);
        results.push({kind: index === 1 ? "foreign_root" : "foreign_store", error: diagnostic, recovered: fixture.recover(error.takePlayer())});
      } finally { fixture.dispose(); }
    }
    return results;
  });
  assert.equal(report.javascript.length, 10);
  for (const row of report.javascript.filter(row => row.kind === "missing" || row.kind === "ambiguous" || row.kind === "cross_root")) {
    assert.ok(row.error.cause, `${row.kind}: semantic cause was flattened`);
  }
  report.liveProperties = await page.evaluate(() => {
    const {livePropertyFixture, livePropertyCases, describe, requireFailure} = window.noonTypedErrorFixtures;
    return livePropertyCases.map(spec => {
      const fixture = livePropertyFixture(spec);
      try {
        const error = requireFailure(fixture.reject, fixture.category);
        fixture.assertDiagnostic(error);
        fixture.assertAtomic();
        return {method: spec.method, kind: spec.kind, axis: spec.axis, error: describe(error), recovered: fixture.recover()};
      } finally { fixture.dispose(); }
    });
  });
  assert.equal(report.liveProperties.length, 29);
  report.python = await page.evaluate(async ({modules, tests, pyodideUrl}) => {
    const wasm = await import("/web/pkg/noon_web.js");
    const {loadPyodide} = await import(pyodideUrl);
    const pyodide = await loadPyodide();
    let store = new wasm.WasmAuthoringStore();
    globalThis.noonCreateCanonicalAuthoringSceneContext = () => store.createSceneContext();
    globalThis.noonCreateAuthoringValueTrackerHandle = initial => store.createValueTracker(initial);
    globalThis.noonAuthoringGeometryOptions = wasm.WasmManimGeometryOptions;
    globalThis.noonAuthoringVectorPath = () => new wasm.WasmAuthoringVectorPath();
    globalThis.noonCreateAuthoringGeometryHandle = options => store.createManimGeometry(options);
    globalThis.noonAuthoringMembershipBatch = kind => new wasm.WasmSceneMembershipBatch(kind);
    globalThis.noonCreateAuthoringFamilyHandle = batch => store.createFamily(batch);
    // Exact plain-result projection from the production worker, no host semantics.
    globalThis.noonResolveAnimationOptions = (...args) => {
      const result = wasm.resolveAnimationOptions(...args);
      try {
        return {ok: result.ok, runTime: result.runTime, rateFunc: result.rateFunc,
          lagRatio: result.lagRatio, pathArc: result.pathArc, reverseRateFunction: result.reverseRateFunction,
          errorKind: result.errorKind ?? "", message: result.message ?? ""};
      } finally { result.free(); }
    };
    globalThis.noonUnmarkedError = () => { throw new Error("foreign handle invalid membership unsupported stale pending"); };
    for (const {runtimePath, source} of modules) pyodide.FS.writeFile(runtimePath, source);
    pyodide.FS.writeFile("/tmp/test_noon_errors_wasm.py", tests);
    pyodide.registerJsModule("_noon_wasm_for_error_tests", wasm);
    pyodide.registerJsModule("_noon_error_test_host", {
      same: (left, right) => left === right, isError: error => error instanceof Error,
      resetStore: () => { store = new wasm.WasmAuthoringStore(); },
      completeAsPromise: async context => context.liveCompleteSegment(),
    });
    return JSON.parse(await pyodide.runPythonAsync(`
import sys, json, unittest
sys.path.insert(0, "/tmp")
from _noon_errors import engine_call
from js import noonTypedErrorFixtures as fixtures, noonUnmarkedError
from pyodide.ffi import JsException
results = []
for kind, live, category, exception_type in [
    ("foreign", False, "foreign_handle", ValueError),
    ("missing", False, "invalid_input", ValueError),
    ("ambiguous", False, "invalid_input", ValueError),
    ("foreign", True, "foreign_handle", ValueError),
    ("missing", True, "invalid_input", ValueError),
    ("pending", True, "pending_work", RuntimeError),
    ("cross_root", False, "unsupported_operation", NotImplementedError),
    ("cross_root", True, "unsupported_operation", NotImplementedError),
]:
    fixture = fixtures.membershipFixture(kind, live)
    try:
        try:
            engine_call(fixture.reject, operation=kind)
        except exception_type as error:
            assert error.category == category, (kind, error.category)
            assert error.code and error.operation == kind and str(error)
            assert error.__cause__ is not None
            if kind in ("missing", "ambiguous", "cross_root"):
                assert error.rust_cause is not None and error.rust_cause.code
            if kind == "cross_root":
                cause = error.rust_cause
                while cause.cause is not None:
                    cause = cause.cause
                assert cause.code == "membership.cross_root_alias"
            fixture.assertAtomic()
            fixture.recover()
            results.append({"kind": kind, "live": live, "category": error.category, "code": error.code})
        else:
            raise AssertionError("invalid membership succeeded")
    finally:
        fixture.dispose()
for index in (1, 2):
    fixture = fixtures.ownershipFixture(index)
    try:
        try:
            engine_call(fixture.reject, operation="returnExecutionPlayer")
        except RuntimeError as error:
            assert error.category == "ownership" and error.code == "ownership.foreign_scene"
            assert callable(error.take_player)
            fixture.assertAtomic()
            fixture.recover(error.take_player())
            results.append({"kind": "ownership", "foreign_index": index, "category": error.category, "code": error.code})
        else:
            raise AssertionError("foreign ownership return succeeded")
    finally:
        fixture.dispose()
try:
    engine_call(noonUnmarkedError)
except JsException as error:
    assert not hasattr(error, "category"), "mapper guessed a category from words"
else:
    raise AssertionError("unmarked JS error was swallowed")
property_results = []
for spec in fixtures.livePropertyCases:
    fixture = fixtures.livePropertyFixture(spec)
    try:
        try:
            engine_call(fixture.reject, operation=spec.method)
        except (ValueError, ReferenceError) as error:
            assert error.category == fixture.category and error.operation == spec.method
            fixture.assertDiagnostic(error.js_error)
            assert error.__cause__ is not None and str(error) == error.js_error.message
            if error.category == "invalid_input":
                assert error.rust_cause.code == "publication.semantic"
                assert error.rust_cause.cause.code == "transaction.non_finite_property_value"
                assert error.rust_cause.cause.cause is None
            fixture.assertAtomic()
            fixture.recover()
            property_results.append({"method": spec.method, "kind": spec.kind, "axis": spec.axis,
                                     "category": error.category, "code": error.code, "recovered": True})
        else:
            raise AssertionError("invalid live property request succeeded")
    finally:
        fixture.dispose()
import test_noon_errors_wasm as tests
result = unittest.TextTestRunner(verbosity=2).run(unittest.defaultTestLoader.loadTestsFromModule(tests))
assert not result.skipped, result.skipped
assert result.wasSuccessful(), "real WASM/Python tests failed"
await tests.check_real_promise_rejection()
json.dumps({"matrix": results, "liveProperties": property_results, "additionalTests": result.testsRun, "promiseRejectionAndRecovery": True, "skipped": len(result.skipped)})
`));
  }, {modules, tests, pyodideUrl});
  assert.equal(report.python.matrix.length, 10);
  assert.equal(report.python.liveProperties.length, 29);
  assert.equal(report.python.additionalTests, 11);
  assert.equal(report.python.skipped, 0);
  assert.equal(report.python.promiseRejectionAndRecovery, true);
  // Exercise actual deployed Python callsites and a rerun in the same worker.
  report.publicPython = await page.evaluate(async () => {
    const {PythonAuthoringClient} = await import("/web/authoring-client.js");
    const client = new PythonAuthoringClient();
    try {
      await client.ready();
      const result = await client.run(`
from noon import Scene, Circle, Square
scene = Scene()
anchor = Circle(radius=0.3)
scene.add(anchor)
missing = Circle(radius=0.2)
replacement = Square(side_length=0.2)
before_members = list(scene.mobjects)
before_id = scene._next_object_id
try:
    scene.replace(missing, replacement)
except ValueError as error:
    assert error.category == "invalid_input"
    assert error.code and error.operation and str(error)
    assert error.rust_cause is not None and error.rust_cause.code
else:
    raise AssertionError("Scene.replace accepted a missing member")
assert list(scene.mobjects) == before_members
assert scene._next_object_id == before_id
assert missing._scene is None and replacement._scene is None
scene.add(replacement)
assert list(scene.mobjects) == [anchor, replacement]
result = scene
`);
      if (!result.semanticExecution || client.terminated) throw new Error("rejection poisoned the worker");
      const recovered = await client.run("from noon import Scene, Circle\nresult = Scene()\nresult.add(Circle(radius=0.2))");
      return {structuredScene: Boolean(result.semanticExecution), subsequentRun: Boolean(recovered.semanticExecution), terminated: client.terminated};
    } finally { client.terminate(); }
  });
  assert.deepEqual(report.publicPython, {structuredScene: true, subsequentRun: true, terminated: false});
  await writeFile(path.join(artifacts, "results.json"), JSON.stringify(report, null, 2));
  console.log(JSON.stringify(report, null, 2));
} catch (error) {
  await writeFile(path.join(artifacts, "failure.txt"), error.stack ?? String(error));
  await writeFile(path.join(artifacts, "partial-results.json"), JSON.stringify(report, null, 2));
  throw error;
} finally {
  await browser?.close();
  await server.close();
  await writeFile(path.join(artifacts, "browser.log"), consoleMessages.join("\n"));
}
