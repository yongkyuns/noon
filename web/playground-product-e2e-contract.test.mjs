import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";
import vm from "node:vm";

const source = await readFile(new URL("../scripts/playground-product-e2e.mjs", import.meta.url), "utf8");

assert.match(source, /const measurement = productMeasurement\(exampleId\);/);
assert.doesNotMatch(source, /executionMetrics\(/);
assert.match(source, /probe\.requestMetrics\(\)/);
assert.match(source, /class ObservedWorker extends NativeWorker/);
assert.match(source, /message\?\.channel !== "noon\.render"/);
assert.match(source, /message\?\.type !== "metrics"/);
assert.match(source, /message\.requestId !== diagnosticRequestId\) return/);
assert.match(source, /event\.stopImmediatePropagation\(\)/);
assert.match(source, /diagnosticRequestId = Number\.MAX_SAFE_INTEGER/);
assert.match(source, /run_time=\$\{measurement\.windowEndSeconds\}/);
assert.match(source, /sha256: createHash\("sha256"\)\.update\(authoredSource\)/);
assert.match(source, /async function synchronizeFinalFrame\(page, seconds\)/);
assert.match(source, /async function waitForRenderedEndpoint\(page, seconds\)/);
assert.match(source, /Math\.abs\(rendered\.time - seconds\) <= 0\.001/);
assert.match(source, /rendered\?\.ready === true/);
assert.match(source, /rendered\.needsPresent === false/);
assert.match(source, /rendered\.bufferedDeltas === 0/);
assert.match(source, /await synchronizeFinalFrame\(page, measurement\.sourceEndSeconds\)/);
assert.match(source, /const screenshotName = "frame-final\.png"/);
assert.match(source, /authoredEndpointSeconds: measurement\.sourceEndSeconds/);
assert.doesNotMatch(source, /frame-0\.5\.png/);
// Counter/clock reset behavior is exercised by playground-product-fps.test.mjs;
// this contract only checks that the real browser harness uses that scorer.
assert.match(source, /sampleRendererFps\(warm\.frameSamples, measurement\.windowEndSeconds/);
assert.match(source, /rendererAt: metrics\?\.sampledAtMs/);
assert.match(source, /clockOriginMs: metrics\?\.performanceTimeOriginMs/);
assert.match(source, /warmupSeconds: measurement\.windowStartSeconds/);
assert.match(source, /samplePresentationGaps\(warm\.presentationSamples, fps\)/);
assert.match(source, /profilePublicationStages: probe\.captureStages/);
assert.match(source, /nonreplayable execution must disable seeking/);
assert.match(source, /probe\.stageKeys\.size >= 2_000/);
assert.match(source, /locator\("\.canvas-frame"\)\.scrollIntoViewIfNeeded\(\)/);
assert.match(source, /minMeasurementMs: MIN_PRODUCT_MEASUREMENT_MS/);
assert.doesNotMatch(source, /editor\.dispatchEvent/);

console.log("✓ product gate scores a warm renderer window and compares the same authored endpoint");

// Exercise the actual browser-init hook with ordinary Worker listeners. Its
// diagnostic replies must never settle a product request or query Python.
function probeHarness() {
  class Worker {
    sent = [];
    listeners = [];
    addEventListener(type, callback) { assert.equal(type, "message"); this.listeners.push(callback); }
    postMessage(message) { this.sent.push(message); }
    emit(data) {
      let stopped = false;
      const event = { data, stopImmediatePropagation() { stopped = true; } };
      for (const callback of this.listeners) {
        callback(event);
        if (stopped) break;
      }
    }
  }
  const window = { Worker };
  const start = source.indexOf("await page.addInitScript(") + "await page.addInitScript(".length;
  const end = source.indexOf("}, measurement.gapClock !== null);", start) + 1;
  assert.ok(end > start, "browser probe hook must be found");
  vm.runInNewContext(`(${source.slice(start, end)})(true)`, { window, performance: { now: () => 100 } });
  return { window, probe: window.__noonProductRenderProbe };
}

function renderReply(requestId, samples = []) {
  return { channel: "noon.render", type: "metrics", requestId, metrics: {
    presentedFrames: 10, time: 3.7, presentedSession: 2,
    performanceTimeOriginMs: 10_000, sampledAtMs: 100, ready: true,
    needsPresent: false, bufferedDeltas: 0, publicationStageSamples: samples,
  } };
}

test("the sampler queries only the renderer and consumes only its own replies", () => {
  const { window, probe } = probeHarness();
  const sourceWorker = new window.Worker("python-worker.js");
  const renderer = new window.Worker("execution-render-worker.js");
  const clientReplies = [];
  renderer.addEventListener("message", event => clientReplies.push(event.data));
  probe.requestMetrics();
  probe.requestMetrics();
  assert.equal(renderer.sent.length, 1, "only one diagnostic request may be in flight");
  assert.equal(renderer.sent[0].requestId, Number.MAX_SAFE_INTEGER);
  assert.equal(renderer.sent[0].profilePublicationStages, false);
  assert.equal(sourceWorker.sent.length, 0, "sampling must not load the callback owner");
  renderer.emit(renderReply(4));
  assert.equal(clientReplies.length, 1);
  assert.equal(probe.pending, true, "ordinary client replies cannot release the probe request");
  renderer.emit(renderReply(Number.MAX_SAFE_INTEGER));
  assert.equal(clientReplies.length, 1, "test replies cannot reach the client request table");
  assert.equal(probe.pending, false);
  assert.equal(probe.metricsReplies, 1);
  assert.equal(probe.latest.frames, 10);
});

test("camera diagnostics deduplicate the bounded worker ring and fail on diagnostic errors", () => {
  const { window, probe } = probeHarness();
  const renderer = new window.Worker("execution-render-worker.js");
  probe.captureStages = true;
  const stage = { session: 2, sequence: 1, presentedAtMs: 99 };
  for (let index = 0; index < 2; index += 1) {
    probe.requestMetrics();
    assert.equal(renderer.sent[index].profilePublicationStages, true);
    renderer.emit(renderReply(Number.MAX_SAFE_INTEGER, [stage]));
  }
  assert.equal(probe.presentationSamples.length, 1);
  assert.equal(probe.presentationSamples[0].clockOriginMs, 10_000);
  probe.requestMetrics();
  assert.throws(() => renderer.emit({ channel: "noon.render", type: "error",
    requestId: Number.MAX_SAFE_INTEGER, message: "test failure" }), /product renderer metrics failed: test failure/);
});
