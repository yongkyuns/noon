import assert from "node:assert/strict";
import { createHash } from "node:crypto";

const INITIAL_MARKER = "  let latestPresentationSample = null;";
const ACK_MARKER = "    acknowledgePresented(displayed);";
const METRICS_MARKER = '            respond(message.requestId, { type: "metrics", metrics });';

const sha256 = (value) => createHash("sha256").update(value).digest("hex");

function matchesHeldCaptureRunInPage(expected) {
  const gallery = window.__noonExampleGallery;
  const diagnostics = gallery?.executionDiagnostics;
  const run = window.__showcaseLiveReviewRun;
  return !run?.stop && gallery?.selectedExampleId === expected.exampleId &&
    gallery?.generationDiagnostics?.runGeneration === expected.runGeneration &&
    diagnostics?.context?.runGeneration === expected.runGeneration &&
    JSON.stringify(diagnostics?.context) === JSON.stringify(expected.context) &&
    diagnostics?.runRequestId === expected.runRequestId &&
    diagnostics?.interpreterGeneration === expected.interpreterGeneration;
}

export function decorateAuthoringRenderController(source, { holdStart, duration }) {
  assert.equal(typeof source, "string");
  assert.ok(Number.isFinite(holdStart) && holdStart >= 0);
  assert.ok(Number.isFinite(duration) && duration > holdStart);
  for (const marker of [INITIAL_MARKER, ACK_MARKER, METRICS_MARKER]) {
    assert.equal(source.split(marker).length - 1, 1, `renderer readback marker must occur exactly once: ${marker}`);
  }
  const initial = `${INITIAL_MARKER}
  let __showcaseRendererReadback = null;
  const __showcaseReadbackView = () => __showcaseRendererReadback === null ? null : ({
    ...__showcaseRendererReadback, pngDataUrl: undefined,
  });
  const __showcaseCaptureRendererReadback = (expected, deadlineEpochMs = null) => {
    const sample = latestPresentationSample;
    const publication = lastPresentedPublication;
    const matches = (value) => value !== null && value.session === expected?.session &&
      value.sequence === expected?.sequence;
    if (__showcaseRendererReadback !== null) {
      const current = __showcaseRendererReadback;
      if (current.session !== expected?.session || current.sequence !== expected?.sequence ||
          current.time !== expected?.time || current.objectCount !== expected?.objectCount) {
        return { status: "error", error: "renderer readback slot belongs to another publication" };
      }
      return __showcaseReadbackView();
    }
    if (stopped || rendererStartupPending || renderPort === null || canvas === null || renderer === null ||
        !matches(publication) || !matches(sample) || sample.time !== expected?.time ||
        renderer.objectCount() !== expected?.objectCount ||
        (deadlineEpochMs !== null && (!Number.isFinite(deadlineEpochMs) ||
          performance.timeOrigin + performance.now() > deadlineEpochMs))) {
      return { status: "error", error: "renderer publication is unavailable or past its capture deadline" };
    }
    const readback = __showcaseRendererReadback = {
      status: "pending", session: sample.session, sequence: sample.sequence, time: sample.time,
      presentedAtMs: sample.presentedAtMs, performanceTimeOriginMs: performance.timeOrigin,
      copyCalledAtEpochMs: performance.timeOrigin + performance.now(), objectCount: expected.objectCount,
      pngDataUrl: null, error: null,
    };
    if (deadlineEpochMs !== null && readback.copyCalledAtEpochMs > deadlineEpochMs) {
      __showcaseRendererReadback = null;
      return { status: "error", error: "renderer copy began after its capture deadline" };
    }
    const rejectReadback = (error) => {
      readback.error = String(error?.message ?? error);
      readback.resolvedAtEpochMs = performance.timeOrigin + performance.now();
      readback.status = "error";
    };
    try {
      if (typeof canvas.convertToBlob !== "function") throw new Error("renderer canvas PNG readback is unavailable");
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
    return __showcaseReadbackView();
  };
  self.__showcaseCaptureRendererReadback = __showcaseCaptureRendererReadback;`;
  const present = `${ACK_MARKER}
      const readbackSample = latestPresentationSample;
      if (__showcaseRendererReadback === null && !rendererStartupPending && renderPort !== null &&
          displayed !== null && readbackSample !== null &&
          readbackSample.session === displayed.session && readbackSample.sequence === displayed.sequence &&
          Number.isFinite(readbackSample.time) && readbackSample.time >= ${JSON.stringify(holdStart)} &&
          readbackSample.time < ${JSON.stringify(duration)}) {
        __showcaseCaptureRendererReadback({ session: readbackSample.session,
          sequence: readbackSample.sequence, time: readbackSample.time, objectCount: renderer.objectCount() });
      }`;
  const metrics = `            if (__showcaseRendererReadback !== null && __showcaseRendererReadback.status !== "pending") {
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
    heldRequests: 0, rejectedHeldRequests: 0 };
  await page.exposeBinding("__showcaseRequestHeldRendererReadback", async ({ page: sourcePage }, request) => {
    report.heldRequests += 1;
    try {
      const workers = sourcePage.workers().filter((worker) =>
        new URL(worker.url()).pathname.endsWith("/execution-render-worker.js"));
      assert.equal(workers.length, 1, "expected exactly one execution render worker for held readback");
      const before = await sourcePage.evaluate(matchesHeldCaptureRunInPage, request);
      assert.equal(before, true, "held renderer request belongs to a stale source Run");
      const readback = await workers[0].evaluate((expected) =>
        self.__showcaseCaptureRendererReadback(expected, expected.deadlineEpochMs), request);
      const after = await sourcePage.evaluate(matchesHeldCaptureRunInPage, request);
      assert.equal(after, true, "held renderer request was cancelled or superseded");
      return readback;
    } catch (error) {
      report.rejectedHeldRequests += 1;
      throw error;
    }
  });
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

export function assertRendererReadback(readback, { attachedPublication, proofReceipt, heldPresentation = null,
  deadlineEpochMs = null, holdStart, duration, minimumObjectCount }) {
  assert.equal(readback?.status, "ready", readback?.error ?? "renderer-owned PNG readback is unavailable");
  assert.match(readback.pngDataUrl ?? "", /^data:image\/png;base64,[A-Za-z0-9+/]+=*$/);
  assert.equal(readback.session, attachedPublication?.session, "renderer readback belongs to a different attached session");
  assert.equal(readback.session, proofReceipt?.session, "renderer readback belongs to a different proof session");
  assert.ok(Number.isSafeInteger(readback.sequence) &&
    readback.sequence >= attachedPublication.sequence && readback.sequence <= proofReceipt.sequence,
  "renderer readback is outside the attached first-pass publication interval");
  const heldMatches = heldPresentation !== null &&
    heldPresentation.publication?.session === proofReceipt?.session &&
    heldPresentation.publication?.sequence === proofReceipt?.sequence &&
    readback.session === heldPresentation.publication.session &&
    readback.sequence === heldPresentation.publication.sequence &&
    heldPresentation.evaluatedTime === proofReceipt?.time && readback.time === heldPresentation.evaluatedTime &&
    heldPresentation.observedTime >= heldPresentation.time && heldPresentation.observedTime < heldPresentation.validUntil &&
    heldPresentation.observedTime >= holdStart && heldPresentation.observedTime < duration;
  assert.ok(Number.isFinite(readback.time) && (heldPresentation !== null
    ? heldMatches
    : readback.time >= holdStart && readback.time < duration),
  "renderer readback is outside the authored terminal hold or exact Rust-held publication");
  assert.ok(Number.isFinite(readback.presentedAtMs) && Number.isFinite(readback.performanceTimeOriginMs) &&
    Number.isFinite(readback.copyCalledAtEpochMs) &&
    readback.copyCalledAtEpochMs >= readback.performanceTimeOriginMs + readback.presentedAtMs &&
    Number.isFinite(readback.copyReturnedAtEpochMs) && readback.copyReturnedAtEpochMs >= readback.copyCalledAtEpochMs &&
    readback.resolvedAtEpochMs >= readback.copyReturnedAtEpochMs,
  "renderer readback timing is incomplete");
  assert.ok(Number.isSafeInteger(readback.objectCount) && readback.objectCount >= minimumObjectCount,
    "renderer readback has an incomplete composition");
  if (heldPresentation !== null) {
    assert.ok(Number.isFinite(deadlineEpochMs) && readback.copyCalledAtEpochMs <= deadlineEpochMs,
      "held renderer copy began after the conservative Rust validity deadline");
  }
}
