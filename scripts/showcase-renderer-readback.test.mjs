import assert from "node:assert/strict";
import test from "node:test";
import { readFile } from "node:fs/promises";
import { runInNewContext } from "node:vm";
import { decorateAuthoringRenderController, assertRendererReadback,
  installAuthoringRendererReadback } from "./showcase-renderer-readback.mjs";

const source = await readFile(new URL("../web/authoring-render-controller.js", import.meta.url), "utf8");
const INITIAL = "  let latestPresentationSample = null;";
const ACK = "    acknowledgePresented(displayed);";
const METRICS = '            respond(message.requestId, { type: "metrics", metrics });';

function fixtureSource() {
  return `${INITIAL}
function respond() {}
const order = [];
let rendererStartupPending = false, renderPort = {};
function acknowledgePresented(displayed) {
  if (rendererStartupPending || displayed === null || renderPort === null) return;
  order.push(["ack", displayed.sequence]);
}
function events() { return order; }
function record(stage, sequence) { order.push([stage, sequence]); }
function tryPresent({ displayedPublication, displayedTime, canvas, sample, startupPending = false, detached = false }) {
  const presentedAtMs = 4, renderer = { objectCount: () => 626 };
  rendererStartupPending = startupPending;
  renderPort = detached ? null : {};
  const displayed = displayedPublication;
  latestPresentationSample = sample ?? { session: displayed.session, sequence: displayed.sequence, time: displayedTime, presentedAtMs };
${ACK}
}
function metrics(message) { const metrics = {};
${METRICS}
  return metrics;
}`;
}

test("controller decoration matches the real controller markers and fails closed when they drift", () => {
  const result = decorateAuthoringRenderController(source, { holdStart: 6.75, duration: 7.5 });
  assert.notEqual(result.sourceSha256, result.instrumentedSha256);
  assert.ok(result.instrumented.length > source.length);
  assert.throws(() => decorateAuthoringRenderController(source.replace(INITIAL, ""),
    { holdStart: 1, duration: 2 }), /occur exactly once/);
  assert.throws(() => decorateAuthoringRenderController(source.replace(ACK, ""),
    { holdStart: 1, duration: 2 }), /occur exactly once/);
  assert.throws(() => decorateAuthoringRenderController(`${source}\n${METRICS}`,
    { holdStart: 1, duration: 2 }), /occur exactly once/);
});

test("ack posts before readback, pending encoding allows later frames, and first in-hold sample stays pinned", async () => {
  const { instrumented } = decorateAuthoringRenderController(fixtureSource(), { holdStart: 6.75, duration: 7.5 });
  let resolveEncoding;
  const context = { performance: { timeOrigin: 100, now: () => 5 }, btoa: () => "aGVsbG8=" };
  runInNewContext(instrumented, context);
  let conversions = 0;
  const canvas = { convertToBlob() {
    conversions += 1;
    assert.equal(JSON.stringify(context.events()), JSON.stringify([["ack", 11]]));
    context.record("copy", 11);
    return new Promise((resolve) => { resolveEncoding = resolve; });
  } };
  context.tryPresent({ displayedPublication: { session: 4, sequence: 11 }, displayedTime: 7, canvas });
  assert.equal(JSON.stringify(context.events()), JSON.stringify([["ack", 11], ["copy", 11]]));
  assert.equal(conversions, 1);
  // A subsequent render proceeds while the first PNG encoding remains unresolved.
  context.tryPresent({ displayedPublication: { session: 4, sequence: 12 }, displayedTime: 7.1, canvas });
  assert.equal(JSON.stringify(context.events()), JSON.stringify([["ack", 11], ["copy", 11], ["ack", 12]]));
  resolveEncoding({ arrayBuffer: () => Promise.resolve(Uint8Array.from([1, 2, 3]).buffer) });
  await new Promise((resolve) => setImmediate(resolve));
  const first = context.metrics({ requestId: 1 }).showcaseRendererReadback;
  const second = context.metrics({ requestId: 2 }).showcaseRendererReadback;
  assert.equal(conversions, 1);
  assert.equal(first.sequence, 11);
  assert.equal(first.time, 7);
  assert.equal(first.copyReturnedAtEpochMs, 105);
  assert.equal(first.pngDataUrl, "data:image/png;base64,aGVsbG8=");
  assert.equal(second.pngDataUrl, first.pngDataUrl);
});

test("synchronous renderer encoding failure is recorded without escaping presentation", () => {
  const { instrumented } = decorateAuthoringRenderController(fixtureSource(), { holdStart: 6.75, duration: 7.5 });
  const context = { performance: { timeOrigin: 100, now: () => 5 }, btoa: () => "" };
  runInNewContext(instrumented, context);
  assert.doesNotThrow(() => context.tryPresent({ displayedPublication: { session: 1, sequence: 2 },
    displayedTime: 7, canvas: { convertToBlob() { throw new Error("encode failed"); } } }));
  assert.equal(JSON.stringify(context.events()), JSON.stringify([["ack", 2]]));
  assert.equal(context.metrics({ requestId: 1 }).showcaseRendererReadback.status, "error");
});

test("readback requires the just-acknowledged receipt and never copies outside the authored hold", () => {
  const { instrumented } = decorateAuthoringRenderController(fixtureSource(), { holdStart: 6.75, duration: 7.5 });
  const context = { performance: { timeOrigin: 100, now: () => 5 }, btoa: () => "" };
  runInNewContext(instrumented, context);
  let conversions = 0;
  const canvas = { convertToBlob() { conversions += 1; return Promise.resolve({ arrayBuffer: () => Promise.resolve(new ArrayBuffer(0)) }); } };
  context.tryPresent({ displayedPublication: { session: 4, sequence: 0 }, displayedTime: 7, startupPending: true, canvas });
  context.tryPresent({ displayedPublication: { session: 4, sequence: 0 }, displayedTime: 7, detached: true, canvas });
  context.tryPresent({ displayedPublication: { session: 4, sequence: 1 }, displayedTime: 6.7, canvas });
  context.tryPresent({ displayedPublication: { session: 4, sequence: 2 }, displayedTime: 7,
    sample: { session: 5, sequence: 2, time: 7, presentedAtMs: 4 }, canvas });
  context.tryPresent({ displayedPublication: { session: 4, sequence: 3 }, displayedTime: 7,
    sample: { session: 4, sequence: 4, time: 7, presentedAtMs: 4 }, canvas });
  context.tryPresent({ displayedPublication: { session: 4, sequence: 4 }, displayedTime: 7.5, canvas });
  assert.equal(conversions, 0);
  context.tryPresent({ displayedPublication: { session: 4, sequence: 3 }, displayedTime: 7, canvas });
  assert.equal(conversions, 1);
  context.tryPresent({ displayedPublication: { session: 5, sequence: 4 }, displayedTime: 7.1, canvas });
  assert.equal(conversions, 1);
});

const valid = {
  status: "ready", session: 4, sequence: 12, time: 7, presentedAtMs: 100,
  performanceTimeOriginMs: 1, copyCalledAtEpochMs: 101, copyReturnedAtEpochMs: 102,
  resolvedAtEpochMs: 120, objectCount: 626,
  pngDataUrl: "data:image/png;base64,aGVsbG8=",
};
const proof = { session: 4, sequence: 13 };
const attached = { session: 4, sequence: 10 };
const args = { attachedPublication: attached, proofReceipt: proof, holdStart: 6.75, duration: 7.5, minimumObjectCount: 600 };

test("renderer readback is joined to the attached session, first-loop proof and authored hold", () => {
  assertRendererReadback(valid, args);
  for (const candidate of [
    { ...valid, session: 5 }, { ...valid, sequence: 9 }, { ...valid, sequence: 14 },
    { ...valid, time: 6.7 }, { ...valid, time: 7.5 }, { ...valid, objectCount: 599 },
    { ...valid, pngDataUrl: null }, { ...valid, status: "pending" }, { ...valid, resolvedAtEpochMs: 100 },
  ]) assert.throws(() => assertRendererReadback(candidate, args));
});

test("held compositor screenshot is bounded by the same before/after Rust publication", async () => {
  const contextId = { exampleId: "spatial", selectionGeneration: 2, runGeneration: 9 };
  const request = { exampleId: "spatial", runGeneration: 9, runRequestId: 18, interpreterGeneration: 3,
    context: contextId, session: 4, sequence: 12, time: 5.95, objectCount: 626,
    holdStart: 6, duration: 8, width: 960, height: 540, deadlineEpochMs: Date.now() + 5000 };
  const heldState = (changes = {}) => ({ time: 7.5, preparedPlayback: true, loopCount: 0,
    playing: true, buffering: false, preparedFrontier: 8, durationSeconds: 8,
    presentationReceipt: { session: 4, sequence: 12, time: 5.95, objectCount: 626 },
    heldPresentation: { publication: { session: 4, sequence: 12 }, evaluatedTime: 5.95,
      time: 7.5, validUntil: 8 }, ...changes });
  const observe = (state) => ({ executionState: state, stateRequestStartedAtEpochMs: Date.now(),
    runStopped: false, selectedExampleId: "spatial", runGeneration: 9, runRequestId: 18,
    interpreterGeneration: 3, context: contextId });
  async function capture(states, bounds = { x: 0, y: 0, width: 960, height: 540 }) {
    let binding;
    const events = [];
    const page = {
      async exposeBinding(_name, callback) { binding = callback; },
      async route() {},
    };
    const instrumentation = await installAuthoringRendererReadback(page, { holdStart: 6, duration: 8 });
    const sourcePage = {
      async evaluate(fn) { return runInNewContext(`(${fn.toString()})(request)`, {
        request, window: { __noonExampleGallery: { async executionState() { return states.shift(); },
          get selectedExampleId() { return "spatial"; },
          generationDiagnostics: { runGeneration: 9 },
          executionDiagnostics: { runRequestId: 18, interpreterGeneration: 3, context: contextId } },
          __showcaseLiveReviewRun: { stop: false } }, Date }); },
      locator() { return { async boundingBox() { return bounds; }, async screenshot(options) {
        events.push(options); return Buffer.from("png");
      } }; },
    };
    const result = await binding({ page: sourcePage }, request);
    return { result, report: instrumentation.report, events };
  }
  const valid = await capture([heldState({ time: 6.05, preparedFrontier: 7, durationSeconds: null,
    heldPresentation: { publication: { session: 4, sequence: 12 }, evaluatedTime: 5.95,
      time: 6.05, validUntil: 7 } }), heldState({ time: 6.1, preparedFrontier: 8,
    heldPresentation: { publication: { session: 4, sequence: 12 }, evaluatedTime: 5.95,
      time: 6.1, validUntil: 8 } })]);
  assert.match(valid.result.pngDataUrl, /^data:image\/png;base64,/);
  assert.equal(valid.events[0].scale, "css");
  assert.deepEqual(valid.result.screenshotBounds, { x: 0, y: 0, width: 960, height: 540 });
  assert.equal(valid.report.heldScreenshotAttempts[0].outcome, "pass");
  for (const [before, after, bounds] of [
    [heldState(), heldState({ presentationReceipt: { session: 4, sequence: 13, time: 5.96, objectCount: 626 } })],
    [heldState({ loopCount: 1 }), heldState()],
    [heldState(), heldState(), { x: 0, y: 0, width: 959, height: 540 }],
  ]) {
    await assert.rejects(capture([before, after], bounds));
  }
});
