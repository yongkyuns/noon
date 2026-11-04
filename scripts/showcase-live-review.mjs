// Review ordinary gallery playback and compare replay against independent
// forward captures. Neither sampled frames nor video timings are an FPS benchmark.
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { mkdir, readFile, writeFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { replayOracle, assertOracleImage, assertReplaySample } from "./showcase-replay-checks.mjs";
import { waitForPublishedGalleryFrame } from "./showcase-playback.mjs";
import { layoutReplayViewport, replayViewport, qualifyReplayViewport } from "./showcase-viewport.mjs";
import { NONREPLAYABLE_SHOWCASE_CAPABILITIES } from "../web/showcase-gallery.js";
import { qualifyGlowViewport } from "./showcase-glow-viewport.mjs";
import { waitForBrowserObservation } from "./playground-browser-support.mjs";
import { assertCaptureTime, validateCaptureIntervals } from "./showcase-capture-checks.mjs";

const REVIEW_CANVAS_VIEWPORT = { width: 960, height: 540 };

// First execution and replay are distinct engine capabilities. Keep both results,
// but do not count a successful first pass as a replacement for failed replay.
export function assertFirstPass(entry, observed, expectedBackend, naturalCapture = null) {
  assert.equal(observed.selectedExampleId, entry.id, "wrong first-pass lesson");
  assert.equal(observed.patchState, "applied", "first-pass source did not finish successfully");
  assert.equal(observed.runInFlight, false, "first-pass source is still running");
  assert.equal(observed.backend, expectedBackend, "wrong first-pass backend");
  const objectCount = naturalCapture?.metrics?.objectCount ?? observed.objectCount;
  assert.ok(Number.isSafeInteger(objectCount) && objectCount > 0,
    "first-pass lesson has no resolved composition");
  if (entry.performance) assert.ok(objectCount >= 600, "dense scene lost its geometry workload");
  if (naturalCapture !== null) {
    if (naturalCapture.heldPresentation) {
      const held = naturalCapture.heldPresentation;
      assertNaturalFirstPassPublication(entry, naturalCapture.authoredDuration, held.observedTime);
      assert.equal(held.publication?.session, naturalCapture.rendererReceipt?.session,
        "held terminal proof belongs to a different renderer session");
      assert.equal(held.publication?.sequence, naturalCapture.rendererReceipt?.sequence,
        "held terminal proof belongs to a different renderer publication");
      assert.ok(held.observedTime >= held.evaluatedTime && held.observedTime < held.validUntil,
        "held terminal proof is outside its Rust-owned validity interval");
    } else {
      assertNaturalFirstPassPublication(entry, naturalCapture.authoredDuration, naturalCapture.metrics?.time);
    }
    const elapsed = Number(naturalCapture.controls?.elapsedSeconds);
    assert.ok(Number.isFinite(elapsed) && elapsed >= naturalCapture.holdStart &&
      elapsed <= naturalCapture.authoredDuration + 8 * Number.EPSILON * Math.max(1, entry.duration),
    "first-pass picture was not captured in the declared terminal hold");
  } else {
    const elapsed = Number(observed.controls?.elapsedSeconds);
    const roundoff = 8 * Number.EPSILON * Math.max(1, entry.duration);
    assert.ok(Number.isFinite(elapsed) && Math.abs(elapsed - entry.duration) <= roundoff,
      "first-pass source did not reach its authored endpoint");
  }
}

export function assertReplayAvailable(observed) {
  assert.equal(observed.replayReason, "", "replayable lesson unexpectedly denied retained replay");
  assert.equal(observed.controls?.controllable, "true",
    observed.replayReason || "completed source did not admit retained replay");
}

export function assertNonreplayableShowcase(entry, observed, expectedBackend, naturalCapture = null) {
  assert.ok(NONREPLAYABLE_SHOWCASE_CAPABILITIES.includes(entry.playbackCapability), "wrong declared nonreplayable capability");
  assert.ok(entry.playbackLimitation?.trim(), "nonreplayable capability lacks its user-facing explanation");
  assertFirstPass(entry, observed, expectedBackend, naturalCapture);
  assert.ok(["Replay unavailable: UnsupportedDomain",
    "Replay unavailable: source execution is not retained for replay"].includes(observed.replayReason),
  "nonreplayable lesson did not report a supported replay denial");
  assert.equal(observed.controls?.controllable, "false", "denied replay controls must be uncontrollable");
  assert.equal(observed.controls?.busy, "false", "completed denied replay must not remain busy");
  assert.deepEqual(observed.disabledReplayControls, { play: true, restart: true, scrubber: true },
    "all replay controls must be disabled after replay denial");
}

export function summarizeLiveReview(results) {
  return {
    firstPassPassed: results.filter((result) => result.firstPassOutcome === "pass").length,
    replayPassed: results.filter((result) => result.replayOutcome === "pass").length,
    expectedNonreplayableDenials: results.filter((result) => result.replayOutcome === "expected-denial").length,
    nonreplayableRerunsPassed: results.filter((result) => result.nonreplayableRerunOutcome === "pass").length,
    unexpectedReplayDenials: results.filter((result) => result.unexpectedReplayDenial === true).length,
    failed: results.filter((result) => result.outcome !== "pass").length,
  };
}

export function assertNaturalFirstPassPublication(entry, authoredDuration, publishedTime) {
  const roundoff = 8 * Number.EPSILON * Math.max(1, entry.duration);
  assert.ok(Number.isFinite(authoredDuration) &&
    Math.abs(authoredDuration - entry.duration) <= roundoff,
  "natural first-pass duration differs from the storyboard");
  const earliest = assertCaptureTime(entry, {
    requestedTime: entry.duration, publishedTime,
  }, entry.duration);
  assert.ok(publishedTime <= authoredDuration + roundoff,
    "natural first-pass publication is after the authored endpoint");
  if (earliest === entry.duration) {
    assert.ok(Math.abs(publishedTime - authoredDuration) <= roundoff,
      "moving first-pass endpoint has stale publication");
  }
}

async function observeNaturalFirstPassWrap({ duration, holdStart, exampleId }) {
  const gallery = window.__noonExampleGallery;
  const patch = document.querySelector("#patch-status");
  if (patch?.dataset.state === "error") throw new Error(patch.value || "Gallery playback failed");
  if (gallery?.selectedExampleId !== exampleId) throw new Error("gallery selection changed during the first pass");
  const diagnostics = gallery?.executionDiagnostics;
  const events = diagnostics?.events ?? [];
  const disposition = events.find((event) => event.stage === "disposition")?.disposition ?? null;
  if (disposition === "looped" || disposition === "finite-complete") return true;
  const heldEvidence = (report, metrics, receipt) => {
    const playback = report?.engineMetrics?.preparedPlayback;
    const observation = playback?.observation;
    const held = observation?.heldPresentation;
    if (!held || observation.loopCount !== 0 || held.publication?.session !== receipt?.session ||
        held.publication?.sequence !== receipt?.sequence || !Number.isFinite(playback.observedTime) ||
        playback.observedTime !== held.time || !Number.isFinite(held.evaluatedTime) ||
        playback.observedTime < held.evaluatedTime || !Number.isFinite(held.validUntil) ||
        playback.observedTime >= held.validUntil || metrics?.needsPresent !== false ||
        metrics?.bufferedDeltas !== 0 || metrics?.time !== receipt?.time ||
        playback.observedTime < holdStart || playback.observedTime >= duration) return null;
    return { ...held, observedTime: playback.observedTime };
  };

  const controls = document.querySelector(".playback-controls")?.dataset;
  const elapsed = Number(controls?.elapsedSeconds);
  const alreadyCaptured = window.__showcaseLiveReviewFirstPass;
  if (!alreadyCaptured && diagnostics?.context?.exampleId === exampleId &&
      Number.isSafeInteger(diagnostics.runRequestId) &&
      Number.isSafeInteger(diagnostics.interpreterGeneration) &&
      events.some((event) => event.stage === "attached" && event.publication) &&
      diagnostics.context?.runGeneration !== null &&
      controls?.busy === "false" && controls.durationKnown === "true" &&
      controls.playing === "true" && Number.isFinite(elapsed) &&
      elapsed >= holdStart && elapsed <= duration + 8 * Number.EPSILON * Math.max(1, duration)) {
    const runIdentity = {
      interpreterGeneration: diagnostics.interpreterGeneration,
      runRequestId: diagnostics.runRequestId,
      context: diagnostics.context,
    };
    const report = await gallery.executionMetrics();
    const current = gallery.executionDiagnostics;
    const currentDisposition = current?.events?.find((event) => event.stage === "disposition")?.disposition ?? null;
    const metrics = report?.metrics;
    const attached = [...(current?.events ?? [])].reverse().find((event) => event.stage === "attached");
    const receipt = metrics?.latestPresentationSample;
    const naturalTerminalReceipt = Number.isFinite(receipt?.time) && receipt.time >= holdStart &&
      receipt.time <= duration + 8 * Number.EPSILON * Math.max(1, duration);
    const heldPresentation = naturalTerminalReceipt ? null : heldEvidence(report, metrics, receipt);
    if (current?.interpreterGeneration === runIdentity.interpreterGeneration &&
        current?.runRequestId === runIdentity.runRequestId &&
        JSON.stringify(current.context) === JSON.stringify(runIdentity.context) &&
        currentDisposition !== "looped" && attached?.publication &&
        attached.publication.session === receipt?.session && Number.isSafeInteger(receipt?.sequence) &&
        receipt.sequence >= attached.publication.sequence && Number.isFinite(receipt.time) &&
        (naturalTerminalReceipt || heldPresentation !== null) &&
        metrics.time === receipt.time && metrics.needsPresent === false && metrics.bufferedDeltas === 0 &&
        Number.isSafeInteger(metrics.objectCount)) {
      const canvas = document.querySelector("#scene");
      if (typeof canvas?.toBlob === "function") {
        // A transferred HTML canvas placeholder can lag the worker's present.
        // Let two main-thread paint updates run, then toBlob synchronously copies
        // the placeholder bitmap before asynchronous PNG serialization (HTML
        // canvas toBlob: https://html.spec.whatwg.org/multipage/canvas.html#dom-canvas-toblob).
        await new Promise((resolve) => requestAnimationFrame(resolve));
        await new Promise((resolve) => requestAnimationFrame(resolve));
        const captureControls = { ...document.querySelector(".playback-controls")?.dataset };
        const captureElapsed = Number(captureControls.elapsedSeconds);
        if (captureControls.busy !== "false" || captureControls.durationKnown !== "true" ||
            captureControls.playing !== "true" || !Number.isFinite(captureElapsed) ||
            captureElapsed < holdStart ||
            captureElapsed > duration + 8 * Number.EPSILON * Math.max(1, duration)) return false;
        const pngBlobPromise = new Promise((resolve) => canvas.toBlob(resolve, "image/png"));
        // Validate the exact run and renderer receipt before awaiting PNG encoding.
        const afterReport = await gallery.executionMetrics();
        const afterMetrics = afterReport?.metrics;
        const afterReceipt = afterMetrics?.latestPresentationSample;
        const afterHeldPresentation = heldPresentation === null ? null :
          heldEvidence(afterReport, afterMetrics, afterReceipt);
        const afterDiagnostics = gallery.executionDiagnostics;
        const afterAttached = [...(afterDiagnostics?.events ?? [])].reverse()
          .find((event) => event.stage === "attached");
        const afterDisposition = afterDiagnostics?.events?.find((event) => event.stage === "disposition")?.disposition ?? null;
        if (afterDiagnostics?.interpreterGeneration === runIdentity.interpreterGeneration &&
            afterDiagnostics?.runRequestId === runIdentity.runRequestId &&
            JSON.stringify(afterDiagnostics.context) === JSON.stringify(runIdentity.context) &&
            afterDisposition !== "looped" &&
            JSON.stringify(afterAttached?.publication) === JSON.stringify(attached.publication) &&
            afterReceipt?.session === receipt.session && afterReceipt.sequence === receipt.sequence &&
            afterReceipt.time === receipt.time && afterMetrics.time === receipt.time &&
            afterMetrics.needsPresent === false && afterMetrics.bufferedDeltas === 0 &&
            afterMetrics.objectCount === metrics.objectCount &&
            (heldPresentation === null ? afterHeldPresentation === null :
              afterHeldPresentation !== null &&
              afterHeldPresentation.publication.session === heldPresentation.publication.session &&
              afterHeldPresentation.publication.sequence === heldPresentation.publication.sequence &&
              afterHeldPresentation.evaluatedTime === heldPresentation.evaluatedTime &&
              afterHeldPresentation.validUntil === heldPresentation.validUntil)) {
          const pngBlob = await pngBlobPromise;
          if (!(pngBlob instanceof Blob)) return false;
          const pngDataUrl = await new Promise((resolve, reject) => {
            const reader = new FileReader();
            reader.onload = () => resolve(reader.result);
            reader.onerror = () => reject(reader.error ?? new Error("terminal canvas PNG read failed"));
            reader.readAsDataURL(pngBlob);
          });
          if (typeof pngDataUrl !== "string" || !pngDataUrl.startsWith("data:image/png;base64,")) return false;
          window.__showcaseLiveReviewFirstPass = {
            pngDataUrl,
            metrics,
            rendererReceipt: receipt,
            heldPresentation,
            controls: captureControls,
            authoredDuration: duration,
            holdStart,
            ...runIdentity,
            attached,
          };
        }
      }
    }
  }
  return false;
}

export async function runOrdinaryGallery(page, deadlineMs = 360000, entry = null) {
  const started = performance.now();
  let timer;
  let firstPassCapture = null;
  let firstPassCompletion = null;
  try {
    await Promise.race([
      (async () => {
        const run = await page.evaluate(() => window.__noonExampleGallery.run());
        const audio = run?.result?.hasAudio === true;
        if (!audio && run?.result?.preparedPlayback !== true) return;
        // Complete finite audio authoring intentionally prepares a silent,
        // paused scene. Exercise the same trusted Play gesture as a viewer;
        // seeking would invalidate the independent first-pass pixel oracle.
        assert.equal(run.stale, false, "gallery Run was superseded");
        if (audio) {
          assert.equal(run.result.time, 0, "finite audio Run did not prepare at time zero");
          assert.equal(run.result.playing, false, "finite audio Run must prepare paused");
        }
        const duration = run.result.durationSeconds;
        assert.ok(Number.isFinite(duration) && (audio ? duration > 0 : duration >= 0),
          `${audio ? "finite audio" : "prepared playback"} Run has no authored duration`);
        if (duration === 0) return;
        const publicationEntry = entry ?? { id: "finite playback", duration, still_intervals: [] };
        validateCaptureIntervals(publicationEntry);
        // Preparation completion can precede the natural playback endpoint.
        // Non-audio prepared Runs already autoplay; preserve that first pass.
        if (audio) await page.getByRole("button", { name: "Play animation", exact: true }).click();
        const roundoff = 8 * Number.EPSILON * Math.max(1, duration);
        const terminalHold = entry?.still_intervals?.find(([, end]) => Math.abs(end - duration) <= roundoff);
        if (!audio && run.result.preparedPlayback === true && terminalHold !== undefined) {
          await waitForBrowserObservation(page, observeNaturalFirstPassWrap, {
            duration,
            holdStart: terminalHold[0],
            exampleId: entry.id,
          }, { timeout: Math.max(1, deadlineMs - (performance.now() - started)) });
          const observed = await page.evaluate(() => {
            const capture = window.__showcaseLiveReviewFirstPass ?? null;
            delete window.__showcaseLiveReviewFirstPass;
            return { capture, diagnostics: window.__noonExampleGallery.executionDiagnostics,
              selectedExampleId: window.__noonExampleGallery.selectedExampleId };
          });
          const disposition = observed.diagnostics?.events?.find((event) => event.stage === "disposition")?.disposition;
          if (disposition === "looped") {
            const capture = observed.capture;
            assert.ok(capture, "first traversal wrapped before its terminal hold picture was captured");
            assert.equal(observed.selectedExampleId, entry.id, "terminal capture belongs to a different gallery selection");
            assert.equal(capture.interpreterGeneration, observed.diagnostics.interpreterGeneration,
              "terminal capture belongs to a different interpreter generation");
            assert.equal(capture.runRequestId, observed.diagnostics.runRequestId,
              "terminal capture belongs to a different source run");
            assert.deepEqual(capture.context, observed.diagnostics.context,
              "terminal capture belongs to a different gallery run context");
            assert.ok(observed.diagnostics.events.some((event) =>
              event.stage === "disposition" && event.disposition === "looped" && event.loopCount >= 1),
            "ordinary first traversal did not complete before the first wrap");
            assert.ok(capture.attached?.publication, "terminal capture has no attached publication identity");
            assert.equal(capture.rendererReceipt?.session, capture.attached.publication.session,
              "terminal picture receipt belongs to a different renderer session than attachment");
            assert.ok(capture.rendererReceipt.sequence >= capture.attached.publication.sequence,
              "terminal picture predates the attached source publication");
            assertNaturalFirstPassPublication(entry, duration,
              capture.heldPresentation?.observedTime ?? capture.metrics.time);
            firstPassCapture = capture;
            firstPassCompletion = {
              disposition: "looped",
              loopCount: observed.diagnostics.events.find((event) =>
                event.stage === "disposition" && event.disposition === "looped")?.loopCount,
              interpreterGeneration: observed.diagnostics.interpreterGeneration,
              runRequestId: observed.diagnostics.runRequestId,
              context: observed.diagnostics.context,
            };
            return;
          }
          // Some explicitly finite prepared hosts complete instead of looping.
          // Retain their existing stopped-endpoint path below.
        }
        while (true) {
          if (performance.now() - started >= deadlineMs) {
            throw new Error("ordinary gallery Run exceeded its review deadline");
          }
          await waitForBrowserObservation(page, ({ duration, requireStopped }) => {
            const patch = document.querySelector("#patch-status");
            if (patch?.dataset.state === "error") throw new Error(patch.value || "Gallery playback failed");
            const gallery = window.__noonExampleGallery;
            const controls = document.querySelector(".playback-controls");
            if (!gallery || gallery.runInFlight || patch?.dataset.state !== "applied" ||
                controls?.dataset.controllable !== "true" || controls.dataset.busy !== "false") return false;
            if (requireStopped ? controls.dataset.playing !== "false" :
                controls.dataset.durationKnown !== "true" || controls.dataset.buffering !== "false") return false;
            const tolerance = 8 * Number.EPSILON * Math.max(1, duration);
            const elapsed = Number(controls.dataset.elapsedSeconds);
            return Number.isFinite(elapsed) && Math.abs(elapsed - duration) <= tolerance;
          }, { duration, requireStopped: audio }, { timeout: Math.max(1, deadlineMs - (performance.now() - started)) });
          const report = await page.evaluate(() => window.__noonExampleGallery.executionMetrics());
          try {
            // Rust's playhead still ends exactly at the authored duration. A
            // declared terminal still interval may retain an earlier visual
            // publication; apply the same policy as the independent captures.
            assertNaturalFirstPassPublication(publicationEntry, duration, report?.metrics?.time);
            break;
          } catch (error) {
            if (!(error instanceof assert.AssertionError)) throw error;
            await page.waitForTimeout(25);
          }
        }
      })(),
      new Promise((_, reject) => {
        timer = setTimeout(() => reject(new Error("ordinary gallery Run exceeded its review deadline")), deadlineMs);
      }),
    ]);
  } finally {
    clearTimeout(timer);
  }
  return { wallMs: performance.now() - started, firstPassCapture, firstPassCompletion };
}

export async function readLiveState(page) {
  return page.evaluate(async () => {
    const metrics = await window.__noonExampleGallery.executionMetrics();
    return {
      selectedExampleId: window.__noonExampleGallery.selectedExampleId,
      runInFlight: window.__noonExampleGallery.runInFlight,
      patchState: document.querySelector("#patch-status")?.dataset.state,
      patch: document.querySelector("#patch-status")?.value,
      backend: document.querySelector("#status")?.dataset.rendererBackend,
      controls: { ...document.querySelector(".playback-controls")?.dataset },
      replayReason: document.querySelector(".playback-controls")?.title,
      disabledReplayControls: {
        play: document.querySelector(".playback-toggle")?.disabled === true,
        restart: document.querySelector(".playback-restart")?.disabled === true,
        scrubber: document.querySelector(".playback-scrubber")?.disabled === true,
      },
      objectCount: metrics?.metrics?.objectCount,
      publishedTime: metrics?.metrics?.time,
      latestPresentationSample: metrics?.metrics?.latestPresentationSample,
    };
  });
}

export function assertLiveOutcome(entry, observed, expectedBackend) {
  assert.equal(observed.selectedExampleId, entry.id, "wrong live lesson");
  assert.equal(observed.patchState, "applied", "source did not finish successfully");
  assert.equal(observed.runInFlight, false, "source is still running");
  assert.equal(observed.backend, expectedBackend, "wrong live backend");
  assert.equal(observed.controls?.controllable, "true", "live lesson cannot replay");
  assert.equal(observed.controls.busy, "false", "playback command is unfinished");
  assert.equal(observed.controls.playing, "false", "capture is not paused");
  const duration = Number(observed.duration);
  const elapsed = Number(observed.controls.elapsedSeconds);
  assert.ok(Number.isFinite(duration) && duration > 0 && Math.abs(duration - entry.duration) < 1e-7,
    "live authored duration differs from the storyboard");
  assert.ok(Number.isFinite(elapsed) && Math.abs(elapsed - duration) < 1e-7,
    "live controls did not reach the authored endpoint");
  assert.ok(Number.isSafeInteger(observed.objectCount) && observed.objectCount > 0,
    "live lesson has no resolved composition");
  if (entry.performance) assert.ok(observed.objectCount >= 600, "dense scene lost its geometry workload");
}

// A live seek targets the actual authored endpoint, not its decimal storyboard.
// Permit only floating-point roundoff between those two representations. Ordinary
// deterministic samples retain their separate, strict assertCaptureTime policy.
export function assertLiveEndpoint(entry, authoredDuration, requestedTime, publishedTime) {
  for (const value of [entry.duration, authoredDuration, requestedTime, publishedTime]) {
    assert.ok(Number.isFinite(value) && value > 0, "invalid live endpoint time");
  }
  const roundoff = 8 * Number.EPSILON * Math.max(1, entry.duration, authoredDuration);
  assert.ok(Math.abs(authoredDuration - entry.duration) <= roundoff,
    "actual duration differs from the storyboard beyond floating-point roundoff");
  assert.equal(requestedTime, authoredDuration, "live seek did not target the actual endpoint");
  assert.ok(Math.abs(publishedTime - authoredDuration) <= roundoff,
    "live endpoint was not presented at its authored time");
}

// Both replay paths must reproduce ordinary execution, not just agree with each other.
export function assertLivePixels(firstPass, replay) {
  assert.equal(replay.width, firstPass.width, "replay width differs from first pass");
  assert.equal(replay.height, firstPass.height, "replay height differs from first pass");
  assert.ok(replay.data.equals(firstPass.data), "replay differs from unseeked first-pass pixels");
}

async function main() {
  const { chromium } = await import("playwright");
  const { PNG } = (await import("pngjs")).default;
  const { serveRepository } = await import("./browser-test-server.mjs");
  const { browserArgs } = await import("./manim-raster-support.mjs");
  const { createPyodideResourceCache } = await import("./pyodide-resource-cache.mjs");
  const { normalizeShowcaseManifest } = await import("../web/showcase-gallery.js");
  const { seekPausedGallery } = await import("./showcase-playback.mjs");
  const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
  const manifest = JSON.parse(await readFile(path.join(root, "web/python/examples/noon_showcase_manifest.json"), "utf8"));
  const selectedIds = process.env.NOON_SHOWCASE_CASES?.split(",").map(id => id.trim()).filter(Boolean);
  if (selectedIds) {
    for (const id of selectedIds) assert.ok(manifest.entries.some(entry => entry.id === id), `unknown showcase: ${id}`);
    manifest.entries = manifest.entries.filter(entry => selectedIds.includes(entry.id));
    assert.ok(manifest.entries.length, "showcase review requires at least one lesson");
  }
  const gallery = normalizeShowcaseManifest(manifest);
  const normalizedById = new Map(gallery.examples.map((entry) => [entry.id, entry]));
  const backend = process.env.NOON_SHOWCASE_BACKEND ?? "webgl";
  assert.ok(["webgl", "webgpu"].includes(backend));
  const expectedBackend = backend === "webgl" ? "WebGL2" : "WebGPU";
  const output = path.join(root, "browser-smoke-artifacts/showcase", backend, "live");
  const hash = (bytes) => createHash("sha256").update(bytes).digest("hex");
  const json = (value) => JSON.stringify(value, (_, item) => typeof item === "bigint" ? String(item) : item, 2);
  const report = {
    purpose: "normal-playback video and replay review; recorded wall time is not a performance benchmark",
    viewport: { width: 1600, height: 1000 }, backendRequested: backend, results: [],
  };
  await mkdir(output, { recursive: true });
  const server = await serveRepository(root, 0);
  const base = `${server.baseUrl}/web`;
  let browser, context;
  try {
    browser = await chromium.launch({ headless: true, args: browserArgs(backend) });
    context = await browser.newContext({
      viewport: report.viewport, deviceScaleFactor: 1,
      recordVideo: { dir: path.join(output, "recordings"), size: report.viewport },
    });
    report.browserVersion = browser.version();
    // Keep collecting ordinary playback even if the independent layout fixture
    // fails. Its result remains fatal to the overall qualification.
    try {
      const shell = await readFile(path.join(root, "web/index.html"), "utf8");
      report.viewportQualification = { outcome: "pass", ...await qualifyReplayViewport(browser, PNG.sync.read, shell) };
    } catch (error) {
      report.viewportQualification = { outcome: "fail", error: String(error.stack ?? error) };
    }
    const cache = createPyodideResourceCache(await readFile(path.join(root, "web/python-worker.js"), "utf8"));
    await cache.install(context);
    const identity = await context.request.get(`${base}/runtime-build-identity.json`);
    assert.ok(identity.ok(), "served build identity is missing");
    report.servedBuildIdentity = await identity.json();
    if (manifest.entries.some(entry => entry.id === "showcase-gpu-glow")) {
      try {
        report.glowViewportQualification = { outcome: "pass", ...await qualifyGlowViewport(
          browser, base, path.join(output, "production-viewport"), backend, cache,
        ) };
      } catch (error) {
        report.glowViewportQualification = { outcome: "fail", error: String(error.stack ?? error) };
      }
    }

    // The capture job runs immediately before this one against the same build.
    // Missing/invalid evidence fails each lesson but must not prevent collecting
    // ordinary-playback videos and existing endpoint diagnostics for the rest.
    let captureReport;
    try {
      const bytes = await readFile(path.join(output, "..", "report.json"));
      captureReport = JSON.parse(bytes);
      report.firstPassCaptureReportSha256 = hash(bytes);
    } catch (error) {
      report.firstPassCaptureReportError = String(error);
    }
    for (const rawEntry of manifest.entries) {
      const entry = { ...rawEntry, ...normalizedById.get(rawEntry.id) };
      const result = { id: entry.id, outcome: "fail", firstPassOutcome: "not-run", replayOutcome: "not-run", intermediateReplayOutcome: "not-run", stage: "open", pageErrors: [], consoleErrors: [] };
      report.results.push(result);
      const page = await context.newPage();
      page.setDefaultTimeout(120000);
      page.on("pageerror", (error) => result.pageErrors.push(String(error.stack ?? error)));
      page.on("console", (message) => {
        if (message.type() === "error") result.consoleErrors.push(message.text());
      });
      const video = page.video();
      try {
        await page.goto(`${base}/index.html?catalog=showcase&example=${entry.id}`);
        await page.waitForFunction(() => window.__noonExampleGallery !== undefined);
        const source = await readFile(path.join(root, "web", entry.path), "utf8");
        const editorSource = await page.locator("#python-scene-source").inputValue();
        assert.equal(editorSource, source, "live review must run the exact checked-in source");
        result.sourceSha256 = hash(source);
        const canvas = page.locator("#scene");
        // Compare ordinary execution and replay against the same backing pixels.
        // Keep the real canvas and renderer; the shared helper only aligns its
        // review layout and replayViewport waits for the host resize to land.
        await layoutReplayViewport(canvas, REVIEW_CANVAS_VIEWPORT);
        result.stage = "ordinary first pass";
        result.firstPassOutcome = "fail";
        // Closing the page in finally retires its workers on timeout; no stuck run survives.
        const firstPassRun = await runOrdinaryGallery(page, 360000, entry);
        result.firstPassWallMsIncludingAuthoring = firstPassRun.wallMs;
        // Check execution before viewport convergence so a runtime panic is not
        // obscured by the renderer's consequently unchanged backing dimensions.
        result.firstPass = await readLiveState(page);
        assertFirstPass(entry, result.firstPass, expectedBackend, firstPassRun.firstPassCapture);
        if (firstPassRun.firstPassCapture !== null) {
          result.firstPassPostWrap = {
            objectCount: result.firstPass.objectCount,
            publishedTime: result.firstPass.publishedTime,
            elapsedSeconds: result.firstPass.controls.elapsedSeconds,
            latestPresentationSample: result.firstPass.latestPresentationSample,
          };
        }
        if (firstPassRun.firstPassCompletion !== null) {
          result.firstPassCompletion = firstPassRun.firstPassCompletion;
        }
        assert.deepEqual(result.pageErrors, [], "ordinary source execution raised browser errors");
        assert.deepEqual(result.consoleErrors, [], "ordinary source execution raised console errors");
        await replayViewport(canvas, REVIEW_CANVAS_VIEWPORT);
        // Preserve first-pass evidence; the manifest below distinguishes expected denial from replay failure.
        const firstPassPath = path.join(output, `${entry.id}-first-pass.png`);
        let firstPassBytes;
        if (firstPassRun.firstPassCapture !== null) {
          firstPassBytes = Buffer.from(firstPassRun.firstPassCapture.pngDataUrl.slice("data:image/png;base64,".length), "base64");
          await writeFile(firstPassPath, firstPassBytes);
          result.firstPassCapture = {
            method: "unseeked terminal hold before first loop wrap",
            metrics: firstPassRun.firstPassCapture.metrics,
            controls: firstPassRun.firstPassCapture.controls,
            interpreterGeneration: firstPassRun.firstPassCapture.interpreterGeneration,
            runRequestId: firstPassRun.firstPassCapture.runRequestId,
            context: firstPassRun.firstPassCapture.context,
            attachedPublication: firstPassRun.firstPassCapture.attached.publication,
            rendererReceipt: firstPassRun.firstPassCapture.rendererReceipt,
            heldPresentation: firstPassRun.firstPassCapture.heldPresentation,
            rendererReceiptScope: "renderer publication sample; does not prove physical scanout",
          };
        } else {
          firstPassBytes = await canvas.screenshot({ path: firstPassPath });
        }
        const firstPass = PNG.sync.read(firstPassBytes);
        result.firstPassPixelSha256 = hash(firstPass.data);
        result.firstPassOutcome = "pass";
        if (entry.playbackCapability !== "deterministic-retained-replay") {
          result.stage = "expected nonreplayable capability";
          result.replayOutcome = "fail";
          assertNonreplayableShowcase(entry, result.firstPass, expectedBackend, firstPassRun.firstPassCapture);
          result.replayOutcome = "expected-denial";
          result.intermediateReplayOutcome = "not-applicable";
          result.stage = "fresh Run after expected replay denial";
          result.nonreplayableRerunOutcome = "fail";
          await layoutReplayViewport(canvas, REVIEW_CANVAS_VIEWPORT);
          const rerun = await runOrdinaryGallery(page, 360000, entry);
          result.nonreplayableRerunWallMsIncludingAuthoring = rerun.wallMs;
          await replayViewport(canvas, REVIEW_CANVAS_VIEWPORT);
          const rerunState = await readLiveState(page);
          assertNonreplayableShowcase(entry, rerunState, expectedBackend, rerun.firstPassCapture);
          const rerunPath = path.join(output, `${entry.id}-nonreplayable-rerun.png`);
          const rerunBytes = rerun.firstPassCapture === null
            ? await canvas.screenshot({ path: rerunPath })
            : Buffer.from(rerun.firstPassCapture.pngDataUrl.slice("data:image/png;base64,".length), "base64");
          if (rerun.firstPassCapture !== null) await writeFile(rerunPath, rerunBytes);
          const rerunImage = PNG.sync.read(rerunBytes);
          result.nonreplayableRerunPixelSha256 = hash(rerunImage.data);
          assertLivePixels(firstPass, rerunImage);
          assert.deepEqual(result.pageErrors, []);
          assert.deepEqual(result.consoleErrors, []);
          result.nonreplayableRerunOutcome = "pass";
          result.nonreplayableRerunMatchesFirstPass = true;
          result.outcome = "pass";
          console.log(`PASS live ${entry.id}: first pass, expected replay denial, and fresh Run endpoint match`);
          continue;
        }
        result.stage = "replay capability admission";
        result.replayOutcome = "fail";
        assertReplayAvailable(result.firstPass);
        result.stage = "replay seek to resolved endpoint";
        const requestedTime = await seekPausedGallery(page, entry.duration);
        const metrics = await waitForPublishedGalleryFrame(page, requestedTime, entry.duration);
        const observed = await page.evaluate(() => ({
          selectedExampleId: window.__noonExampleGallery.selectedExampleId,
          runInFlight: window.__noonExampleGallery.runInFlight,
          patchState: document.querySelector("#patch-status")?.dataset.state,
          backend: document.querySelector("#status")?.dataset.rendererBackend,
          controls: { ...document.querySelector(".playback-controls")?.dataset },
          duration: document.querySelector(".playback-scrubber")?.max,
        }));
        observed.objectCount = metrics.metrics.objectCount;
        assertLiveOutcome(entry, observed, expectedBackend);
        assertLiveEndpoint(entry, Number(observed.duration), requestedTime, metrics.metrics.time);
        result.observed = observed;
        result.requestedTime = requestedTime;
        result.publishedTime = metrics.metrics.time;
        const endpoint = PNG.sync.read(await canvas.screenshot({
          path: path.join(output, `${entry.id}-endpoint.png`),
        }));
        result.endpointPixelSha256 = hash(endpoint.data);
        assert.ok(endpoint.width >= 320 && endpoint.height >= 180, "live canvas is too small");
        assertLivePixels(firstPass, endpoint);
        result.endpointMatchesFirstPass = true;
        result.stage = "restart and recover endpoint";
        await page.getByRole("button", { name: "Restart animation from the beginning", exact: true }).click();
        const replayTime = await seekPausedGallery(page, entry.duration);
        const replayMetrics = await waitForPublishedGalleryFrame(page, replayTime, entry.duration);
        assertLiveEndpoint(entry, Number(observed.duration), replayTime, replayMetrics.metrics.time);
        const replay = PNG.sync.read(await canvas.screenshot({
          path: path.join(output, `${entry.id}-restart.png`),
        }));
        result.restartPixelSha256 = hash(replay.data);
        assertLivePixels(firstPass, replay);
        result.restartMatchesFirstPass = true;
        assert.deepEqual(result.pageErrors, []);
        result.restartRestoresEndpoint = true;
        result.stage = "independent intermediate replay qualification";
        result.intermediateReplayOutcome = "fail";
        const oracle = replayOracle(entry, captureReport, {
          sourceSha256: result.sourceSha256, buildIdentity: report.servedBuildIdentity,
          browserVersion: report.browserVersion, backendRequested: backend,
        });
        // Reaffirm the oracle's integer-aligned viewport before intermediate
        // samples; all captures use the original canvas pixels without resampling.
        await layoutReplayViewport(canvas, captureReport.viewport);
        await seekPausedGallery(page, entry.duration, 0);
        await waitForPublishedGalleryFrame(page, 0, entry.duration);
        result.intermediateViewport = await replayViewport(canvas, captureReport.viewport);
        result.intermediateSamples = [];
        for (const [direction, checkpoints] of [["backward", [...oracle].reverse()], ["forward", oracle]]) {
          for (const [index, checkpoint] of checkpoints.entries()) {
            const comparison = { direction, firstPassFilename: checkpoint.filename,
              firstPassRequestedTime: checkpoint.requestedTime, firstPassPublishedTime: checkpoint.publishedTime,
              firstPassPixelSha256: checkpoint.pixelSha256, requestedTime: checkpoint.replayTime, outcome: "fail" };
            result.intermediateSamples.push(comparison);
            try {
              const firstBytes = await readFile(path.join(output, "..", checkpoint.filename));
              const firstImage = PNG.sync.read(firstBytes);
              assertOracleImage(checkpoint, firstBytes, firstImage);
              const time = await seekPausedGallery(page, entry.duration,
                checkpoint.completionProbe ? null : checkpoint.replayTime);
              const replayMetrics = await waitForPublishedGalleryFrame(page, time, entry.duration);
              comparison.publishedTime = replayMetrics.metrics.time;
              comparison.backend = replayMetrics.metrics.backend;
              comparison.filename = `${entry.id}-${direction}-${index}.png`;
              const image = PNG.sync.read(await canvas.screenshot({ path: path.join(output, comparison.filename) }));
              comparison.pixelSha256 = hash(image.data);
              comparison.width = image.width;
              comparison.height = image.height;
              assertReplaySample(entry, checkpoint, time, replayMetrics.metrics);
              assertLivePixels(firstImage, image);
              comparison.outcome = "pass";
            } catch (error) {
              comparison.error = String(error.stack ?? error);
            }
          }
        }
        assert.deepEqual(result.pageErrors, []);
        assert.deepEqual(result.consoleErrors, []);
        assert.deepEqual(result.intermediateSamples.filter(sample => sample.outcome !== "pass")
          .map(sample => `${sample.direction}@${sample.requestedTime}`), [], "intermediate replay differs from first execution");
        result.intermediateReplayOutcome = "pass";
        result.replayOutcome = "pass";
        result.outcome = "pass";
        console.log(`PASS live ${entry.id}: normal source run, replay endpoint, and ${result.intermediateSamples.length} intermediate comparisons`);
      } catch (error) {
        result.error = String(error.stack ?? error);
        result.unexpectedReplayDenial = entry.playbackCapability === "deterministic-retained-replay" &&
          result.firstPass?.controls?.controllable === "false";
        result.failureState = await page.evaluate(() => ({
          patch: document.querySelector("#patch-status")?.value,
          status: document.querySelector("#status-text")?.textContent,
          controls: { ...document.querySelector(".playback-controls")?.dataset },
        })).catch(() => null);
        await page.screenshot({ path: path.join(output, `${entry.id}-failure.png`) }).catch(() => {});
        console.error(`FAIL live ${entry.id}: ${result.error}`);
      } finally {
        await page.close();
        try {
          assert.ok(video, "review video was not created");
          result.video = `${entry.id}.webm`;
          await video.saveAs(path.join(output, result.video));
          await video.delete();
        } catch (error) {
          result.outcome = "fail";
          result.videoError = String(error);
        }
        await writeFile(path.join(output, "report.json"), json(report));
      }
    }
    assert.notEqual(report.glowViewportQualification?.outcome, "fail", "production-layout glow projection failed");
    report.runtimeResources = cache.stats();
    report.summary = summarizeLiveReview(report.results);
    assert.equal(report.viewportQualification.outcome, "pass", "browser capture layout fixture failed");
    assert.deepEqual(report.results.filter((result) => result.outcome !== "pass").map((result) => result.id), [],
      "normal-playback review failures");
  } finally {
    await writeFile(path.join(output, "report.json"), json(report));
    await context?.close();
    await browser?.close();
    await server.close();
  }
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) await main();
