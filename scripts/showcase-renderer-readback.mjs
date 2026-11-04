import assert from "node:assert/strict";
import { createHash } from "node:crypto";

const INITIAL_MARKER = "  let latestPresentationSample = null;";
const ACK_MARKER = "    acknowledgePresented(displayed);";
const METRICS_MARKER = '            respond(message.requestId, { type: "metrics", metrics });';

const sha256 = (value) => createHash("sha256").update(value).digest("hex");

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
  const report = { requestCount: 0, error: null, sourceSha256: null, instrumentedSha256: null };
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
