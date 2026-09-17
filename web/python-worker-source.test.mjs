import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

const source = await readFile(new URL("./python-worker.source.js", import.meta.url), "utf8");

test("Python authoring worker keeps request validation helper", () => {
  assert.match(source, /function\s+validateRequest\s*\(/);
  assert.doesNotMatch(source, /validateHostRequest|attach_engine_port|runCallbackPhase/);
  assert.match(source, /function\s+isRecord\s*\(value\)\s*\{/);
  assert.match(source, /if\s*\(!isRecord\(request\)\s*\|\|\s*request\.channel\s*!==\s*AUTHORING_CHANNEL\)/);
});

test("Python authoring worker routes specialized geometry through common typed admission", () => {
  assert.match(source, /noonAuthoringGeometryOptions = WasmManimGeometryOptions/);
  assert.match(source, /noonCreateAuthoringGeometryHandle = \(options\) =>\s*\n?\s*authoringStore\.createManimGeometry\(options\)/);
  for (const shape of [
    "Dot", "Triangle", "Elbow", "RoundedRectangle", "AnnularSector",
    "Sector", "Annulus", "DashedLine", "Underline",
  ]) {
    assert.doesNotMatch(source, new RegExp(`noonCreateAuthoring${shape}Handle`));
    assert.doesNotMatch(source, new RegExp(`authoringStore\\.createManim${shape}\\(`));
    assert.doesNotMatch(source, new RegExp(`manim${shape}SnapshotJson`));
  }
});

test("generic geometry inputs cross the optional wrapper as typed values", () => {
  assert.match(source, /noonAuthoringGeometryOptions = WasmManimGeometryOptions/);
  assert.match(source, /noonAuthoringVectorPath = \(\) => new WasmAuthoringVectorPath\(\)/);
  assert.match(source, /authoringStore\.createManimGeometry\(options\)/);
  assert.doesNotMatch(source, /noonCreateAuthoringMobjectHandle|\.createMobject\(/);
});

test("image inputs cross the worker as typed retained resources with bounded URL bytes", () => {
  assert.match(source, /WasmImageMobjectOptions/);
  assert.match(source, /noonAuthoringImageOptions\s*=\s*WasmImageMobjectOptions/);
  assert.match(source, /noonCreateAuthoringImageHandle\s*=\s*\(options\)\s*=>\s*authoringStore\.createImage\(options\)/);
  assert.match(source, /const limit = 32 \* 1024 \* 1024/);
  assert.match(source, /response\.body\.getReader\(\)/);
  assert.match(source, /size > limit/);
});

test("detached ValueTracker construction stays in the shared authoring store", async () => {
  assert.match(
    source,
    /noonCreateAuthoringValueTrackerHandle\s*=\s*\(initial\)\s*=>\s*\n?\s*authoringStore\.createValueTracker\(initial\)/,
  );
  const example = await readFile(
    new URL("./python/examples/ordinary_value_tracker_continuation.py", import.meta.url),
    "utf8",
  );
  assert.match(example, /^progress = ValueTracker\(0\.0\)$/m);
  assert.match(example, /self\.bind_position\(\s*circle, progress,/);
  assert.doesNotMatch(example, /self\.value_tracker\(/);
});

test("semantic continuation control bypasses the blocked interpreter request queue", () => {
  assert.match(source, /if\s*\(isContinuationControl\(event\.data\)\)/);
  assert.match(source, /void\s+handleContinuationControl\(event\.data\)/);
  assert.match(source, /requestQueue\s*=\s*requestQueue\.then\(\(\)\s*=>\s*handleRequest/);
  assert.match(
    source,
    /await\s+execute_construct\(\s*__noon_result,\s*portable_constructs=__noon_portable_constructs,\s*\)/,
  );
  assert.match(source, /continuation\.endpoint\.startContinuation\(continuation\.generation\)/);
  assert.match(source, /continuation\.runRequestId\s*!==\s*request\.continuationRunRequestId/);
  assert.match(source, /noonRequireSemanticContinuationActive/);
  const lane = source.slice(
    source.indexOf("async function handleContinuationControl"),
    source.indexOf("async function handleRequest"),
  );
  assert.doesNotMatch(lane, /runPythonAsync/);
  const queuedLane = source.slice(
    source.indexOf("async function handleRequest"),
    source.indexOf("async function attachSemanticExecutionRequest"),
  );
  assert.doesNotMatch(queuedLane, /release_semantic_execution/);
});

test("releasing a prior context bypasses a fresh continuation without retiring its owner", async () => {
  const start = source.indexOf("function isContinuationControl");
  const end = source.indexOf("async function handleRequest", start);
  assert.ok(start >= 0 && end > start, "continuation control boundaries must exist");
  const controlsSource = source.slice(start, end);
  const prior = { released: false, endpoints: new Set() };
  const fresh = { released: false, endpoints: new Set() };
  const contexts = new Map([["prior", prior], ["fresh", fresh]]);
  const freshContinuation = {
    contextId: "fresh",
    generation: 7,
    runRequestId: 41,
    terminal: false,
  };
  const posts = [];
  const errors = [];
  const retired = [];
  const controls = new Function(
    "AUTHORING_CHANNEL", "isRecord", "validateRequest", "pyodidePromise",
    "attachSemanticExecutionRequest", "semanticContexts", "activeAuthoringRun",
    "retireSemanticContext", "post", "postError", "failContinuation", `
      ${controlsSource}
      return { isContinuationControl, handleContinuationControl };
    `,
  )(
    "noon.authoring",
    (value) => typeof value === "object" && value !== null && !Array.isArray(value),
    (request) => assert.equal(request.channel, "noon.authoring"),
    Promise.resolve({}),
    async () => assert.fail("release must not attach a fresh endpoint"),
    contexts,
    { continuation: freshContinuation },
    (contextId, entry) => {
      retired.push({ contextId, entry });
      if (entry.released && entry.endpoints.size === 0) contexts.delete(contextId);
    },
    (type, payload) => posts.push({ type, ...payload }),
    (requestId, error) => errors.push({ requestId, message: String(error.message ?? error) }),
    () => assert.fail("release must not cancel a fresh continuation"),
  );

  let finishQueuedRun;
  const queuedRun = new Promise((resolve) => { finishQueuedRun = resolve; });
  let queuedRunFinished = false;
  void queuedRun.then(() => { queuedRunFinished = true; });
  let requestQueue = queuedRun;
  const dispatch = (request) => {
    if (controls.isContinuationControl(request)) return controls.handleContinuationControl(request);
    requestQueue = requestQueue.then(() => assert.fail("unrelated release must not enter interpreter queue"));
    return requestQueue;
  };

  const release = dispatch({
    channel: "noon.authoring",
    type: "release_semantic_execution",
    requestId: 9,
    contextId: "prior",
  });
  await release;
  assert.equal(queuedRunFinished, false, "prior-context release must not wait for the fresh source");
  assert.equal(prior.released, true);
  assert.deepEqual(retired, [{ contextId: "prior", entry: prior }]);
  assert.equal(contexts.has("prior"), false, "the released prior token must be retired");
  assert.deepEqual(posts, [{ type: "semantic_execution_released", requestId: 9 }]);
  assert.deepEqual(errors, []);
  assert.equal(contexts.get("fresh"), fresh);
  assert.equal(fresh.released, false);
  assert.equal(freshContinuation.terminal, false);

  await controls.handleContinuationControl({
    channel: "noon.authoring",
    type: "release_semantic_execution",
    requestId: 10,
    contextId: "fresh",
  });
  assert.deepEqual(errors, [{
    requestId: 10,
    message: "cannot release an active semantic continuation context",
  }]);
  assert.equal(fresh.released, false);
  assert.equal(freshContinuation.terminal, false);
  finishQueuedRun();
  await queuedRun;
});

test("semantic continuation delivers required callback work to its suspended source", () => {
  assert.match(source, /noonSetSemanticContinuationCallbackSession/);
  assert.match(source, /noonCompleteSemanticContinuationCallback/);
  assert.match(source, /noonFailSemanticContinuationCallback/);
  assert.match(source, /continuation\.callbackRequest/);
  assert.match(source, /pending\.resolve\(continuationEvent\("callback",\s*\{ phase \}\)\)/);
  assert.match(source, /callback\.resolve\(patchBatchJson\)/);
  assert.match(source, /callback\.reject\(new Error\(message\)\)/);
  assert.match(source, /continuationOnly\s*\?\s*\(frame\)\s*=>\s*requestContinuationCallback/);
});

test("suspended callback reads stay token-pinned and cannot settle after cancellation", () => {
  assert.match(source, /noonReadSemanticContinuationCallback/);
  assert.match(source, /function\s+readContinuationCallback\s*\(/);
  assert.match(source, /continuationCallbackRequest\(context, tokenJson\)/);
  assert.match(source, /callback\.read !== null/);
  assert.match(source, /continuation\.callbackRead\(tokenJson, request\)/);
  assert.match(source, /if \(callback\.read !== null\) callback\.read\.reject\(failure\)/);
  assert.match(source, /semantic continuation callback cannot complete while a callback read is pending/);
});

test("sparse callback proof keeps scalar and inactive-object reads in its updater", async () => {
  const example = await readFile(
    new URL("./python/examples/ordinary_callback_sparse_reads.py", import.meta.url),
    "utf8",
  );
  assert.match(example, /anchor\.get_center\(\)/);
  assert.match(example, /tracker\.get_value\(\)/);
  assert.match(example, /await self\.wait\(0\.25\)/);
  assert.match(example, /phase_counts\[phase_time\].*== 1/);
});

test("worker delegates every Scene construct lifecycle to the canonical adapter", () => {
  const authoring = source.slice(
    source.indexOf("async function runAuthoringSource"),
    source.indexOf("async function runCanonicalCallbackPhase"),
  );
  assert.match(
    authoring,
    /await\s+execute_construct\(\s*__noon_result,\s*portable_constructs=__noon_portable_constructs,\s*\)/,
  );
  assert.doesNotMatch(authoring, /__noon_result\.(?:setup|construct|tear_down)\(/);
  assert.doesNotMatch(authoring, /exportDocument|to_document|to_scene_spec|materialize_legacy_geometry/);
  assert.doesNotMatch(authoring, /_(?:begin|finish)_(?:async|synchronous)_continuation_construct/);
});


test("retired callback sessions release only after the active Python run unwinds", async () => {
  const start = source.indexOf("function retireSemanticContext");
  const end = source.indexOf("async function runAuthoringSource", start);
  assert.ok(start >= 0 && end > start, "retirement function boundaries must exist");
  const retirementSource = source.slice(start, end);
  let finishRun;
  const activeRun = new Promise((resolve) => { finishRun = resolve; });
  let releases = 0;
  const contexts = new Map();
  const entry = { released: true, endpoints: new Set(), releaseCallbackSession() { releases += 1; } };
  contexts.set("context", entry);
  const retire = new Function("semanticContexts", "postError", "activeRun", `
    let requestQueue = activeRun;
    ${retirementSource}
    return (token, entry) => { retireSemanticContext(token, entry); return requestQueue; };
  `)(contexts, (id, error) => { throw error; }, activeRun);
  const released = retire("context", entry);
  await Promise.resolve();
  assert.equal(contexts.size, 0);
  assert.equal(releases, 0);
  finishRun();
  await released;
  assert.equal(releases, 1);
});


test("fatal interpreter rejection is forwarded once and closes the dead worker", () => {
  const handlerSource = source.slice(
    source.indexOf('self.addEventListener("unhandledrejection"'),
    source.indexOf('self.addEventListener("message"'),
  );
  const handlers = new Map();
  const errors = [];
  let closes = 0;
  let prevented = 0;
  const scope = {
    addEventListener(name, handler) { handlers.set(name, handler); },
    close() { closes += 1; },
  };
  new Function("self", "postError", `let fatalAuthoringFailure = false; ${handlerSource}`)(
    scope, (requestId, error) => errors.push({ requestId, error }),
  );
  const error = new Error("interpreter suspension failed");
  const event = { reason: error, preventDefault() { prevented += 1; } };
  handlers.get("unhandledrejection")(event);
  handlers.get("unhandledrejection")(event);
  assert.deepEqual(errors, [{ requestId: null, error }]);
  assert.equal(closes, 1);
  assert.equal(prevented, 2);
  assert.match(source, /if \(fatalAuthoringFailure\) return;/);
});

test("source compilation never replays module effects or changes fixtures", () => {
  assert.match(source, /compile_authoring_source\(\s*__noon_source\s*\)/);
  assert.match(source, /await execute_authoring_module\(__noon_code, __noon_namespace\)/);
  assert.match(source, /__noon_namespace\[MODULE_BARRIER_GLOBAL\] = await_module_source_barrier/);
  assert.doesNotMatch(source, /exec\(__noon_source,|source\.replace/);
  assert.match(source, /__noon_namespace\[BARRIER_GLOBAL\] = await_source_barrier/);
});

test("explicit endpoint reconnect returns the existing runtime lease before reattaching", async () => {
  const attachSource = source.slice(source.indexOf("async function attachSemanticExecutionRequest"), source.indexOf("function retireSemanticContext"));
  const contexts = new Map();
  const entry = { context: {}, endpoints: new Set(), released: false };
  contexts.set("scene", entry);
  let leased = true;
  const old = { stop() { leased = false; entry.endpoints.delete(old); } };
  entry.endpoints.add(old);
  const run = { continuation: null };
  const attach = new Function("semanticContexts", "activeAuthoringRun", "attachSemanticEngine", `
    ${attachSource}
    return attachSemanticExecutionRequest;
  `)(contexts, run, async () => {
    if (leased) throw new Error("runtime already leased");
    leased = true;
    return { stop() {} };
  });
  await assert.rejects(attach({ contextId: "scene" }, false, null), /already leased/);
  assert.equal(entry.endpoints.has(old), true, "ordinary attachment cannot steal a live runtime");
  run.continuation = { contextId: "scene", terminal: false };
  await assert.rejects(attach({ contextId: "scene", replaceExistingEndpoint: true }, false, null), /continuation is active/);
  assert.equal(entry.endpoints.has(old), true);
  run.continuation = null;
  await attach({ contextId: "scene", replaceExistingEndpoint: true }, false, null);
  assert.equal(entry.endpoints.has(old), false);
  assert.equal(entry.endpoints.size, 1);
  assert.equal(leased, true);
});
