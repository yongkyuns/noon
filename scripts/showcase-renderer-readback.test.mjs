import assert from "node:assert/strict";
import test from "node:test";
import { readFile } from "node:fs/promises";
import { runInNewContext } from "node:vm";
import { decorateAuthoringRenderController, assertRendererReadback } from "./showcase-renderer-readback.mjs";

const source = await readFile(new URL("../web/authoring-render-controller.js", import.meta.url), "utf8");
const INITIAL = "  let latestPresentationSample = null;";
const PRESENT = "      if (stageSample !== null) stageSample.time = displayedTime;";
const METRICS = '            respond(message.requestId, { type: "metrics", metrics });';

test("controller decoration matches the real controller markers and fails closed when they drift", () => {
  const result = decorateAuthoringRenderController(source, { holdStart: 6.75, duration: 7.5 });
  assert.notEqual(result.sourceSha256, result.instrumentedSha256);
  assert.ok(result.instrumented.length > source.length);
  assert.throws(() => decorateAuthoringRenderController(source.replace(INITIAL, ""),
    { holdStart: 1, duration: 2 }), /occur exactly once/);
  assert.throws(() => decorateAuthoringRenderController(`${source}\n${METRICS}`,
    { holdStart: 1, duration: 2 }), /occur exactly once/);
});

test("decorated render captures once and its ready PNG survives repeated UI metrics reads", async () => {
  const fixture = `${INITIAL}
function respond() {}
function tryPresent({ displayedPublication, displayedTime, canvas }) {
  const presentedAtMs = 4, stageSample = null, renderer = { objectCount: () => 626 };
  if (displayedPublication !== null) {
${PRESENT}
  }
}
function metrics(message) { const metrics = {};
${METRICS}
  return metrics;
}`;
  const { instrumented } = decorateAuthoringRenderController(fixture, { holdStart: 6.75, duration: 7.5 });
  const context = { performance: { timeOrigin: 100, now: () => 5 }, btoa: () => "aGVsbG8=" };
  runInNewContext(instrumented, context);
  let conversions = 0;
  const canvas = { convertToBlob() {
    conversions += 1;
    return Promise.resolve({ arrayBuffer: () => Promise.resolve(Uint8Array.from([1, 2, 3]).buffer) });
  } };
  context.tryPresent({ displayedPublication: { session: 4, sequence: 11 }, displayedTime: 7, canvas });
  context.tryPresent({ displayedPublication: { session: 4, sequence: 12 }, displayedTime: 7.1, canvas });
  await new Promise((resolve) => setImmediate(resolve));
  const first = context.metrics({ requestId: 1 }).showcaseRendererReadback;
  const second = context.metrics({ requestId: 2 }).showcaseRendererReadback;
  assert.equal(conversions, 1);
  assert.equal(first.sequence, 11);
  assert.equal(first.pngDataUrl, "data:image/png;base64,aGVsbG8=");
  assert.equal(second.pngDataUrl, first.pngDataUrl);
});

test("synchronous renderer encoding failure is recorded without escaping presentation", () => {
  const fixture = `${INITIAL}
function respond() {}
function tryPresent({ displayedPublication, displayedTime, canvas }) {
  const presentedAtMs = 4, stageSample = null, renderer = { objectCount: () => 7 };
  if (displayedPublication !== null) {
${PRESENT}
  }
}
function metrics(message) { const metrics = {};
${METRICS}
  return metrics;
}`;
  const { instrumented } = decorateAuthoringRenderController(fixture, { holdStart: 6.75, duration: 7.5 });
  const context = { performance: { timeOrigin: 100, now: () => 5 }, btoa: () => "" };
  runInNewContext(instrumented, context);
  assert.doesNotThrow(() => context.tryPresent({ displayedPublication: { session: 1, sequence: 2 },
    displayedTime: 7, canvas: { convertToBlob() { throw new Error("encode failed"); } } }));
  assert.equal(context.metrics({ requestId: 1 }).showcaseRendererReadback.status, "error");
});

const valid = {
  status: "ready", session: 4, sequence: 12, time: 7, presentedAtMs: 100,
  performanceTimeOriginMs: 1, copyCalledAtEpochMs: 101, resolvedAtEpochMs: 120, objectCount: 626,
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
