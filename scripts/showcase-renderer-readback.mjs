import assert from "node:assert/strict";
import { createHash } from "node:crypto";

const INITIAL_MARKER = "  let latestPresentationSample = null;";
const ACK_MARKER = "    acknowledgePresented(displayed);";
const METRICS_MARKER = '            respond(message.requestId, { type: "metrics", metrics });';

const sha256 = (value) => createHash("sha256").update(value).digest("hex");

async function readHeldScreenshotState() {
  const gallery = window.__noonExampleGallery;
  const stateRequestStartedAtEpochMs = Date.now();
  const executionState = await gallery.executionState();
  const diagnostics = gallery.executionDiagnostics;
  return { executionState, stateRequestStartedAtEpochMs,
    runStopped: window.__showcaseLiveReviewRun?.stop === true,
    selectedExampleId: gallery.selectedExampleId,
    runGeneration: gallery.generationDiagnostics?.runGeneration,
    runRequestId: diagnostics?.runRequestId,
    interpreterGeneration: diagnostics?.interpreterGeneration,
    context: diagnostics?.context };
}

function assertHeldScreenshotState(snapshot, expected) {
  const state = snapshot.executionState;
  const receipt = state?.presentationReceipt;
  const held = state?.heldPresentation;
  assert.equal(snapshot.runStopped, false, "held screenshot Run was cancelled");
  assert.equal(snapshot.selectedExampleId, expected.exampleId, "held screenshot belongs to a different lesson");
  assert.equal(snapshot.runGeneration, expected.runGeneration, "held screenshot belongs to a stale Run generation");
  assert.equal(snapshot.runRequestId, expected.runRequestId, "held screenshot belongs to a stale source Run");
  assert.equal(snapshot.interpreterGeneration, expected.interpreterGeneration, "held screenshot belongs to a stale interpreter");
  assert.deepEqual(snapshot.context, expected.context, "held screenshot belongs to a different run context");
  assert.equal(state?.preparedPlayback, true);
  assert.equal(state?.loopCount, 0);
  assert.equal(state?.playing, true);
  assert.equal(state?.buffering, false);
  assert.ok(Number.isFinite(state?.time) && state.time >= expected.holdStart && state.time < expected.duration);
  assert.ok(Number.isFinite(state?.preparedFrontier) && state.preparedFrontier >= state.time);
  assert.ok(state?.durationSeconds == null || (Number.isFinite(state.durationSeconds) &&
    Math.abs(state.durationSeconds - expected.duration) <= 8 * Number.EPSILON * Math.max(1, expected.duration)));
  assert.equal(receipt?.session, expected.session);
  assert.equal(receipt?.sequence, expected.sequence);
  assert.equal(receipt?.time, expected.time);
  assert.equal(receipt?.objectCount, expected.objectCount);
  assert.equal(held?.publication?.session, expected.session);
  assert.equal(held?.publication?.sequence, expected.sequence);
  assert.equal(held?.evaluatedTime, expected.time);
  assert.equal(held?.time, state.time);
  assert.ok(Number.isFinite(held?.validUntil) && state.time < held.validUntil);
  return held;
}

async function captureHeldCompositedScreenshot(page, request, report) {
  const attempt = { session: request.session, sequence: request.sequence, time: request.time,
    deadlineEpochMs: request.deadlineEpochMs, error: null };
  report.heldScreenshotAttempts.push(attempt);
  try {
    const before = await page.evaluate(readHeldScreenshotState);
    attempt.before = before;
    const beforeHeld = assertHeldScreenshotState(before, request);
    const deadline = Math.min(request.deadlineEpochMs,
      before.stateRequestStartedAtEpochMs + (beforeHeld.validUntil - before.executionState.time) * 1000);
    const canvas = page.locator("#scene");
    const bounds = await canvas.boundingBox();
    assert.ok(bounds && Math.abs(bounds.width - request.width) < 0.01 &&
      Math.abs(bounds.height - request.height) < 0.01,
    "held screenshot canvas does not match the authored pixel viewport");
    attempt.bounds = bounds;
    attempt.startedAtEpochMs = Date.now();
    assert.ok(attempt.startedAtEpochMs < deadline, "held compositor screenshot started after the Rust validity deadline");
    const screenshot = await canvas.screenshot({ type: "png", scale: "css",
      timeout: Math.max(1, Math.floor(deadline - attempt.startedAtEpochMs)) });
    attempt.returnedAtEpochMs = Date.now();
    assert.ok(attempt.returnedAtEpochMs <= deadline, "held compositor screenshot returned after the Rust validity deadline");
    const after = await page.evaluate(readHeldScreenshotState);
    attempt.after = after;
    const afterHeld = assertHeldScreenshotState(after, request);
    assert.ok(attempt.startedAtEpochMs >= before.stateRequestStartedAtEpochMs &&
      after.stateRequestStartedAtEpochMs >= attempt.returnedAtEpochMs,
    "held screenshot is not bracketed by Rust state observations");
    assert.ok(after.executionState.time >= before.executionState.time &&
      afterHeld.validUntil >= beforeHeld.validUntil,
    "held screenshot Rust observation moved backward");
    attempt.outcome = "pass";
    return { pngDataUrl: `data:image/png;base64,${screenshot.toString("base64")}`,
      screenshotStartedAtEpochMs: attempt.startedAtEpochMs,
      screenshotReturnedAtEpochMs: attempt.returnedAtEpochMs,
      screenshotBounds: bounds, before, after };
  } catch (error) {
    attempt.error = String(error?.stack ?? error);
    throw error;
  }
}

export function decorateAuthoringRenderController(source, { holdStart, duration }) {
  assert.equal(typeof source, "string");
  assert.ok(Number.isFinite(holdStart) && holdStart >= 0);
  assert.ok(Number.isFinite(duration) && duration > holdStart);
  for (const marker of [INITIAL_MARKER, ACK_MARKER, METRICS_MARKER]) {
    assert.equal(source.split(marker).length - 1, 1, `renderer readback marker must occur exactly once: ${marker}`);
  }
  const initial = `${INITIAL_MARKER}\n  let __showcaseRendererReadback = null;`;
  const present = `${ACK_MARKER}
      const readbackSample = latestPresentationSample;
      if (__showcaseRendererReadback === null && !rendererStartupPending && renderPort !== null &&
          displayed !== null && readbackSample !== null &&
          readbackSample.session === displayed.session && readbackSample.sequence === displayed.sequence &&
          Number.isFinite(readbackSample.time) && readbackSample.time >= ${JSON.stringify(holdStart)} &&
          readbackSample.time < ${JSON.stringify(duration)}) {
        const readback = __showcaseRendererReadback = {
          status: "pending", session: readbackSample.session, sequence: readbackSample.sequence,
          time: readbackSample.time, presentedAtMs: readbackSample.presentedAtMs,
          performanceTimeOriginMs: performance.timeOrigin,
          copyCalledAtEpochMs: performance.timeOrigin + performance.now(), objectCount: null, pngDataUrl: null, error: null,
        };
        const rejectReadback = (error) => {
          readback.error = String(error?.message ?? error);
          readback.resolvedAtEpochMs = performance.timeOrigin + performance.now();
          readback.status = "error";
        };
        try {
          if (typeof canvas?.convertToBlob !== "function") throw new Error("renderer canvas PNG readback is unavailable");
          readback.objectCount = renderer.objectCount();
          const encoding = canvas.convertToBlob({ type: "image/png" });
          readback.copyReturnedAtEpochMs = performance.timeOrigin + performance.now();
          encoding.then(async (blob) => {
            const bytes = new Uint8Array(await blob.arrayBuffer());
            let binary = "";
            for (let offset = 0; offset < bytes.length; offset += 0x8000) {
              binary += String.fromCharCode(...bytes.subarray(offset, Math.min(offset + 0x8000, bytes.length)));
            }
            readback.pngDataUrl = "data:image/png;base64," + btoa(binary);
            readback.resolvedAtEpochMs = performance.timeOrigin + performance.now();
            readback.status = "ready";
          }).catch(rejectReadback);
        } catch (error) {
          rejectReadback(error);
        }
      }`;
  const metrics = `            if (__showcaseRendererReadback !== null) {
              metrics.showcaseRendererReadback = __showcaseRendererReadback;
            }` + "\n" + METRICS_MARKER;
  const instrumented = source.replace(INITIAL_MARKER, initial)
    .replace(ACK_MARKER, present)
    .replace(METRICS_MARKER, metrics);
  return { instrumented, sourceSha256: sha256(source), instrumentedSha256: sha256(instrumented) };
}

export async function installAuthoringRendererReadback(page, { holdStart, duration }) {
  let resolveServed;
  let rejectServed;
  const served = new Promise((resolve, reject) => { resolveServed = resolve; rejectServed = reject; });
  const report = { requestCount: 0, error: null, sourceSha256: null, instrumentedSha256: null,
    heldScreenshotAttempts: [] };
  await page.exposeBinding("__showcaseCaptureHeldCompositedScreenshot", async ({ page: sourcePage }, request) =>
    captureHeldCompositedScreenshot(sourcePage, request, report));
  await page.route("**/authoring-render-controller.js", async (route) => {
    report.requestCount += 1;
    try {
      assert.equal(report.requestCount, 1, "renderer controller was requested more than once");
      const response = await route.fetch();
      assert.ok(response.ok(), `renderer controller request failed: ${response.status()}`);
      const source = await response.text();
      const decorated = decorateAuthoringRenderController(source, { holdStart, duration });
      report.sourceSha256 = decorated.sourceSha256;
      report.instrumentedSha256 = decorated.instrumentedSha256;
      resolveServed();
      await route.fulfill({ response, body: decorated.instrumented });
    } catch (error) {
      report.error = String(error?.stack ?? error);
      rejectServed(error);
      await route.abort().catch(() => {});
    }
  });
  served.catch(() => {});
  return {
    report,
    async assertServed(timeoutMs = 10000) {
      let timer;
      try {
        await Promise.race([served, new Promise((_, reject) => {
          timer = setTimeout(() => reject(new Error("renderer controller instrumentation was not served")), timeoutMs);
        })]);
      } finally {
        clearTimeout(timer);
      }
      assert.equal(report.requestCount, 1);
      assert.match(report.sourceSha256 ?? "", /^[a-f0-9]{64}$/);
      assert.match(report.instrumentedSha256 ?? "", /^[a-f0-9]{64}$/);
      assert.notEqual(report.sourceSha256, report.instrumentedSha256);
    },
  };
}

export function assertRendererReadback(readback, { attachedPublication, proofReceipt, holdStart, duration, minimumObjectCount }) {
  assert.equal(readback?.status, "ready", readback?.error ?? "renderer-owned PNG readback is unavailable");
  assert.match(readback.pngDataUrl ?? "", /^data:image\/png;base64,[A-Za-z0-9+/]+=*$/);
  assert.equal(readback.session, attachedPublication?.session, "renderer readback belongs to a different attached session");
  assert.equal(readback.session, proofReceipt?.session, "renderer readback belongs to a different proof session");
  assert.ok(Number.isSafeInteger(readback.sequence) &&
    readback.sequence >= attachedPublication.sequence && readback.sequence <= proofReceipt.sequence,
  "renderer readback is outside the attached first-pass publication interval");
  assert.ok(Number.isFinite(readback.time) && readback.time >= holdStart && readback.time < duration,
    "renderer readback is outside the authored terminal hold");
  assert.ok(Number.isFinite(readback.presentedAtMs) && Number.isFinite(readback.performanceTimeOriginMs) &&
    Number.isFinite(readback.copyCalledAtEpochMs) &&
    readback.copyCalledAtEpochMs >= readback.performanceTimeOriginMs + readback.presentedAtMs &&
    Number.isFinite(readback.copyReturnedAtEpochMs) && readback.copyReturnedAtEpochMs >= readback.copyCalledAtEpochMs &&
    readback.resolvedAtEpochMs >= readback.copyReturnedAtEpochMs,
  "renderer readback timing is incomplete");
  assert.ok(Number.isSafeInteger(readback.objectCount) && readback.objectCount >= minimumObjectCount,
    "renderer readback has an incomplete composition");
}

// A delayed main-thread observer may first see the loop after the renderer has
// already copied the terminal picture. Rust's first-wrap publication bounds that
// copy without inferring playback time from wall clocks or accepting a later loop.
export function assertReadbackBeforeFirstWrap(readback, firstWrap, args) {
  assert.equal(firstWrap?.disposition, "looped");
  assert.equal(firstWrap.loopCount, 1, "readback proof missed the first loop");
  assert.ok(Number.isFinite(firstWrap.time) && firstWrap.time >= 0 && firstWrap.time < args.holdStart,
    "first-wrap observation reached a later terminal hold");
  assert.ok(Number.isFinite(firstWrap.publication?.time) && firstWrap.publication.time >= 0 &&
    firstWrap.publication.time < args.holdStart, "first-wrap publication reached a later terminal hold");
  assertRendererReadback(readback, { ...args, proofReceipt: firstWrap.publication });
  assert.ok(readback.sequence < firstWrap.publication.sequence,
    "terminal readback does not precede the first-wrap publication");
}
