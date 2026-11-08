import assert from "node:assert/strict";
import test from "node:test";
import { runInNewContext } from "node:vm";
import { assertFirstPass, assertReplayAvailable, assertNonreplayableShowcase, assertLiveOutcome, assertLiveEndpoint, assertLivePixels, assertNaturalFirstPassPublication, summarizeLiveReview, runOrdinaryGallery, readLiveState, observeNaturalFirstPassWrap } from "./showcase-live-review.mjs";
import { holdPlaygroundPreload } from "./playground-browser-support.mjs";

const entry = { id: "showcase-example", duration: 8 };
function observed() {
  return {
    selectedExampleId: entry.id, patchState: "applied", runInFlight: false,
    backend: "WebGPU", objectCount: 3, duration: "7.999999999999999",
    controls: { controllable: "true", busy: "false", playing: "false", elapsedSeconds: "7.999999999999999" },
  };
}

test("live review accepts the real authored endpoint without rewriting it", () => {
  const state = observed();
  const before = structuredClone(state);
  assertLiveOutcome(entry, state, "WebGPU");
  assert.deepEqual(state, before);
});

test("incomplete, misrouted, wrong-backend or unavailable live playback fails", () => {
  for (const change of [
    { selectedExampleId: "different" }, { patchState: "error" }, { runInFlight: true },
    { backend: "WebGL2" }, { objectCount: 0 }, { objectCount: NaN },
    { duration: "" }, { duration: "NaN" }, { duration: "7.9" },
  ]) assert.throws(() => assertLiveOutcome(entry, { ...observed(), ...change }, "WebGPU"));
  for (const change of [
    { controllable: "false" }, { busy: "true" }, { playing: "true" },
    { elapsedSeconds: "0" }, { elapsedSeconds: "NaN" },
  ]) {
    const state = observed();
    Object.assign(state.controls, change);
    assert.throws(() => assertLiveOutcome(entry, state, "WebGPU"));
  }
});

test("normal-playback evidence cannot silently reduce the dynamic workload", () => {
  const dynamic = { ...entry, performance: true };
  assert.throws(() => assertLiveOutcome(dynamic, observed(), "WebGPU"));
  assertLiveOutcome(dynamic, { ...observed(), objectCount: 626 }, "WebGPU");
});

// Values observed in the failed 3343197 CI artifacts on both backends.
test("live endpoint accepts rounding above or below the decimal storyboard without relabeling", () => {
  for (const [duration, actual] of [
    [23.4, 23.400000000000002], [7.6, 7.6000000000000005],
    [2.6, 2.5999999999999996], [8, 7.999999999999999], [8, 8],
  ]) {
    assertLiveEndpoint({ duration }, actual, actual, actual);
  }
});

test("live endpoint rejects stale pixels, the wrong request and substantive duration drift", () => {
  const duration = 23.400000000000002;
  for (const args of [
    [duration, duration, duration - 0.001],
    [duration, duration, duration + 0.001],
    [duration, 23.4, duration],
    [duration + 0.001, duration + 0.001, duration + 0.001],
    [duration, duration, 21.8],
  ]) assert.throws(() => assertLiveEndpoint({ duration: 23.4 }, ...args));
});

test("live endpoint rejects missing or coerced timestamps", () => {
  for (const value of [undefined, null, NaN, Infinity, -1, 0, "8"]) {
    for (let index = 0; index < 4; index++) {
      const times = [8, 8, 8, 8]; times[index] = value;
      assert.throws(() => assertLiveEndpoint({ duration: times[0] }, ...times.slice(1)));
    }
  }
});

function pixels(data = [0, 10, 20, 255, 30, 40, 50, 255]) {
  return { width: 2, height: 1, data: Buffer.from(data) };
}

test("both replay paths must reproduce independent unseeked pixels", () => {
  const firstPass = pixels();
  const before = Buffer.from(firstPass.data);
  assertLivePixels(firstPass, pixels());
  assertLivePixels(firstPass, pixels());
  assert.ok(firstPass.data.equals(before));
});

test("two identical wrong replays cannot substitute for the first pass", () => {
  const firstPass = pixels();
  const endpoint = pixels();
  endpoint.data[0] ^= 1;
  const restart = { ...endpoint, data: Buffer.from(endpoint.data) };
  // This is the old replay-versus-replay check: it passes despite the regression.
  assert.ok(endpoint.data.equals(restart.data));
  for (const replay of [endpoint, restart]) {
    assert.throws(() => assertLivePixels(firstPass, replay), /unseeked first-pass pixels/);
  }
});

test("live pixel equality rejects dimensions and every changed RGBA channel", () => {
  const firstPass = pixels();
  assert.throws(() => assertLivePixels(firstPass, { ...pixels(), width: 1, height: 2 }));
  assert.throws(() => assertLivePixels(firstPass, { ...pixels(), height: 2 }));
  assert.throws(() => assertLivePixels(firstPass, { ...pixels(), data: Buffer.alloc(0) }));
  for (let i = 0; i < firstPass.data.length; i++) {
    const changed = pixels();
    changed.data[i] ^= 1;
    assert.throws(() => assertLivePixels(firstPass, changed), /unseeked first-pass pixels/);
  }
});


test("successful first execution cannot substitute for unavailable replay", () => {
  const state = observed();
  state.controls.controllable = "false";
  state.replayReason = "Replay unavailable: UnsupportedDomain";
  assertFirstPass(entry, state, "WebGPU");
  assert.throws(() => assertReplayAvailable(state), /UnsupportedDomain/);
  assert.throws(() => assertLiveOutcome(entry, state, "WebGPU"));
});

test("declared nonreplayable lessons require a known idle replay denial", () => {
  const entry = {
    id: "showcase-reactive-relationships", duration: 9.2,
    playbackCapability: "nonreplayable-host-callbacks",
    playbackLimitation: "Python updater callbacks cannot be retained for deterministic replay.",
  };
  const state = {
    ...observed(),
    selectedExampleId: entry.id,
    duration: "9.199999999999999",
    replayReason: "Replay unavailable: UnsupportedDomain",
    controls: { controllable: "false", busy: "false", elapsedSeconds: "9.199999999999999" },
    disabledReplayControls: { play: true, restart: true, scrubber: true },
  };
  assertNonreplayableShowcase(entry, state, "WebGPU");
  assertNonreplayableShowcase(entry, {
    ...state, replayReason: "Replay unavailable: source execution is not retained for replay",
  }, "WebGPU");
  for (const change of [
    { replayReason: "Replay unavailable: UnsupportedFeature" },
    { controls: { ...state.controls, controllable: "true" } },
    { controls: { ...state.controls, busy: "true" } },
    { disabledReplayControls: { play: true, restart: false, scrubber: true } },
  ]) assert.throws(() => assertNonreplayableShowcase(entry, { ...state, ...change }, "WebGPU"));
  const anotherLesson = { ...entry, id: "showcase-other-callback-lesson" };
  assert.doesNotThrow(() => assertNonreplayableShowcase(anotherLesson, {
    ...state, selectedExampleId: anotherLesson.id,
  }, "WebGPU"));
  const nativeInput = { ...entry, id: "showcase-translation-drag", playbackCapability: "nonreplayable-native-input" };
  assert.doesNotThrow(() => assertNonreplayableShowcase(nativeInput, {
    ...state, selectedExampleId: nativeInput.id,
  }, "WebGPU"));
  assert.throws(() => assertNonreplayableShowcase({ ...nativeInput, playbackCapability: "deterministic-retained-replay" }, state, "WebGPU"));
});

test("unexpected denial stays fatal and expected nonreplayable reruns have separate counters", () => {
  assert.throws(() => assertReplayAvailable({
    controls: { controllable: "false" }, replayReason: "Replay unavailable: UnsupportedDomain",
  }), /unexpectedly denied retained replay/);
  assert.deepEqual(summarizeLiveReview([
    { outcome: "pass", firstPassOutcome: "pass", replayOutcome: "pass" },
    { outcome: "pass", firstPassOutcome: "pass", replayOutcome: "expected-denial", nonreplayableRerunOutcome: "pass" },
    { outcome: "fail", firstPassOutcome: "pass", replayOutcome: "fail", unexpectedReplayDenial: true },
  ]), {
    firstPassPassed: 3, replayPassed: 1, expectedNonreplayableDenials: 1,
    nonreplayableRerunsPassed: 1, unexpectedReplayDenials: 1, failed: 1,
  });
});

test("ordinary Run helper applies the same deadline and state snapshot to each execution", async () => {
  const { page, calls } = preparedAudioPage({ priorPhase: { runGeneration: 7, exampleId: "showcase-example" },
    run: { stale: false, result: { hasAudio: false, time: 0 } } });
  assert.ok(Number.isFinite((await runOrdinaryGallery(page, 100)).wallMs));
  await assert.rejects(runOrdinaryGallery({ evaluate: () => new Promise(() => {}) }, 5), /review deadline/);
  assert.deepEqual(calls, ["run"]);

  const state = { selectedExampleId: entry.id, controls: { controllable: "true" }, objectCount: 3 };
  let stateCalls = 0;
  assert.deepEqual(await readLiveState({ evaluate: async () => { stateCalls += 1; return state; } }), state);
  assert.equal(stateCalls, 1);
});

test("first-pass review rejects a Run that already started during automatic preload", async () => {
  for (const options of [
    { priorRunInFlight: true },
    { priorPhase: { runGeneration: 8, exampleId: "showcase-example" } },
  ]) {
    const { page, calls } = preparedAudioPage(options);
    await assert.rejects(runOrdinaryGallery(page, 100, null, { expectColdStart: true }),
      /(?:review Run started while another Run was active|first-pass review page already accepted a Run)/);
    assert.deepEqual(calls, []);
  }
});

test("explicit first-pass review holds autorun bootstrap until its page closes", async () => {
  let routeHandler;
  let closed = false;
  let continued = false;
  const release = await holdPlaygroundPreload({
    isClosed: () => closed,
    async route(url, handler) {
      assert.equal(url, "**/live-authoring-bootstrap.js");
      routeHandler = handler;
    },
  });
  const blockedRequest = routeHandler({ async continue() { continued = true; } });
  await Promise.resolve();
  assert.equal(continued, false, "autorun bootstrap passed the held module boundary");
  closed = true;
  release();
  await blockedRequest;
  assert.equal(continued, false, "closing the page released a second automatic Run");
});

function preparedAudioPage({ run, runDelayMs = 0, priorRunInFlight = false,
  priorPhase = null, rejectBeforeGeneration = null, selectedExampleId = "showcase-example", observations = [] } = {}) {
  const calls = [], ready = [];
  let observationIndex = 0;
  let metricsCalls = 0;
  let generation = 8;
  let runPhaseMetrics = priorPhase;
  let current = {};
  const gallery = {
    selectedExampleId,
    run() {
      calls.push("run");
      if (rejectBeforeGeneration) throw rejectBeforeGeneration;
      generation += 1;
      runPhaseMetrics = { runGeneration: generation, exampleId: selectedExampleId };
      return (async () => {
        if (runDelayMs) {
          await new Promise((resolve) => setTimeout(resolve, runDelayMs));
          observations[0]?.captureEvents?.push("run-resolved");
        }
        return run ?? { stale: false, result: { hasAudio: true, time: 0, playing: false, durationSeconds: 8 } };
      })();
    },
    get generationDiagnostics() { return { runGeneration: generation }; },
    get runPhaseMetrics() { return runPhaseMetrics; },
    get runInFlight() { return current.runInFlight ?? priorRunInFlight; },
    async executionMetrics() { return observationReport("metrics"); },
    async executionState() {
      const report = observationReport("state");
      const playback = report.engineMetrics?.preparedPlayback;
      const observation = playback?.observation;
      return { preparedPlayback: playback !== undefined, time: playback?.observedTime,
        preparedFrontier: observation?.frontier, durationSeconds: observation?.end,
        heldPresentation: observation?.heldPresentation, loopCount: current.looped ? 1 : observation?.loopCount,
        playing: true, buffering: false,
        presentationReceipt: report.metrics.needsPresent || report.metrics.bufferedDeltas
          ? null : { ...report.metrics.latestPresentationSample, objectCount: report.metrics.objectCount } };
    },
    get executionDiagnostics() {
      const base = current.diagnostics ?? {
        schemaVersion: 1, interpreterGeneration: 3, runRequestId: 17,
        context: { exampleId: "showcase-example", selectionGeneration: 2,
          runGeneration: current.contextRunGeneration ?? 9 },
        events: [{ stage: "attached", publication: current.attachedPublication ?? {
          session: current.session ?? 7, sequence: 4 } }],
      };
      return current.looped ? { ...base, events: [...base.events,
        { stage: "disposition", disposition: "looped", loopCount: 1 }] } : base;
    },
  };
  function observationReport(owner) {
      metricsCalls += 1;
      current.captureEvents?.push(`${owner}-${metricsCalls}`);
      if (current.wrapOnFreshMetrics) current.looped = true;
      const sample = current.postPresentationSample && metricsCalls === 1
        ? current.postPresentationSample
        : current.presentationSample ?? {
          session: current.session ?? 7, sequence: current.sequence ?? 5,
          time: current.publishedTime, presentedAtMs: 100,
        };
      const supplied = current.preparedPlayback;
      return { metrics: { time: sample.time, objectCount: current.objectCount ?? 55,
        needsPresent: current.needsPresent ?? false, bufferedDeltas: current.bufferedDeltas ?? 0,
        latestPresentationSample: sample },
      ...(current.omitPreparedPlayback ? {} : { engineMetrics: { preparedPlayback: {
        ...(supplied ?? {}),
        observedTime: supplied?.observedTime ?? current.observedTime ?? current.publishedTime,
        observation: { frontier: current.frontier ?? current.elapsed,
          end: current.end === null ? null : current.end ?? 8,
          ...(supplied?.observation ?? {}), loopCount: supplied?.observation?.loopCount ?? current.rustLoopCount ?? 0 },
      } } }) };
  }
  class TestBlob { constructor(dataUrl) { this.dataUrl = dataUrl; } }
  class TestFileReader {
    readAsDataURL(blob) {
      this.result = blob.dataUrl;
      setTimeout(() => this.onload?.(), 0);
    }
  }
  const browser = {
    window: { __noonExampleGallery: gallery,
      __showcaseCaptureHeldCompositedScreenshot: async (request) => {
        current.captureEvents?.push("held-compositor-screenshot");
        return current.heldScreenshotResult ?? null;
      } },
    document: { querySelector(selector) {
      if (selector === "#patch-status") return {
        dataset: { state: current.patchState ?? "applied" }, value: current.patchError,
      };
      if (selector === "#status") return { dataset: { rendererBackend: "WebGL2" } };
      if (selector === "#scene") return {
        toBlob(callback) {
          const copiedDataUrl = current.canvasDataUrl ?? "data:image/png;base64,aGVsbG8=";
          current.captureEvents?.push("toBlob-copy");
          if (current.wrapSynchronouslyDuringToBlob) {
            current.captureEvents?.push("wrap-during-toBlob");
            current.looped = true;
          }
          if (current.advanceReceiptDuringToBlob) current.sequence = (current.sequence ?? 5) + 1;
          setTimeout(() => {
            if (current.wrapDuringBlobEncoding) {
              current.captureEvents?.push("wrap-during-encoding");
              current.looped = true;
            }
            current.captureEvents?.push("toBlob-serialize");
            callback(new TestBlob(copiedDataUrl));
          }, 0);
        },
      };
      if ([".playback-toggle", ".playback-restart", ".playback-scrubber"].includes(selector)) {
        return { disabled: false };
      }
      assert.equal(selector, ".playback-controls");
      return { dataset: { controllable: "true", busy: "false", playing: "false",
        durationKnown: "true", buffering: "false",
        elapsedSeconds: String(current.elapsed), ...current.controls } };
    } },
  };
  const evaluate = async (fn, argument) => {
    if (argument?.exampleId || argument?.requireStopped !== undefined) {
      if (observationIndex === 0) calls.push("published endpoint");
      current = observations[observationIndex++] ?? {};
    }
    const value = await runInNewContext(`(${fn.toString()})(argument)`, {
      ...browser,
      argument,
      Blob: TestBlob,
      FileReader: TestFileReader,
      performance: { timeOrigin: 0, now: () => Date.now() },
      requestAnimationFrame(callback) {
        current.captureEvents?.push("requestAnimationFrame");
        if (current.wrapDuringAnimationFrame) {
          current.looped = true;
          current.elapsed = current.publishedTime = 0.1;
        }
        callback(0);
        return 1;
      },
      setTimeout,
      clearTimeout,
    });
    if (argument?.exampleId || argument?.requireStopped !== undefined) ready.push(value);
    return value;
  };
  return {
    calls, ready, window: browser.window,
    page: {
      evaluate,
      getByRole(role, options) {
        assert.equal(role, "button");
        assert.deepEqual(options, { name: "Play animation", exact: true });
        return { async click() { calls.push("trusted Play"); } };
      },
      async waitForTimeout(milliseconds) {
        assert.equal(milliseconds, 25);
        await new Promise((resolve) => setTimeout(resolve, 1));
      },
      waitForFunction() { throw new Error("async worker observations must poll their resolved value"); },
    },
  };
}

test("finite audio first pass uses trusted Play and waits for the unseeked published endpoint", async () => {
  const { page, calls, ready } = preparedAudioPage({ observations: [
    { elapsed: 0, publishedTime: 0 },
    { elapsed: 4, publishedTime: 4, controls: { playing: "true" } },
    { elapsed: 8, publishedTime: 8, controls: { busy: "true" } },
    { elapsed: 8, publishedTime: 8, runInFlight: true },
    { elapsed: 8, publishedTime: 8, controls: { controllable: "false" } },
    { elapsed: 7.999, publishedTime: 8 },
    { elapsed: 8, publishedTime: 7.999 },
    { elapsed: 8 },
    { elapsed: 8, publishedTime: 8 },
  ] });
  assert.ok(Number.isFinite((await runOrdinaryGallery(page, 100)).wallMs));
  assert.deepEqual(calls, ["run", "trusted Play", "published endpoint"]);
  assert.deepEqual(ready, [false, false, false, false, false, false, true, true, true]);
});

test("natural finite playback accepts a declared terminal hold and retains its real publication", async () => {
  const publication = 29.012699999999256; // Observed settled WebGL frame with the Rust/UI clock at 30.
  const lesson = { id: "showcase-background-music", duration: 30, still_intervals: [[29, 30]] };
  const { page, calls, ready } = preparedAudioPage({
    selectedExampleId: "showcase-background-music",
    run: { stale: false, result: { hasAudio: true, time: 0, playing: false, durationSeconds: 30 } },
    observations: [
      { elapsed: 29, publishedTime: publication },
      { elapsed: 30, publishedTime: 28.99 },
      { elapsed: 30, publishedTime: 30.01 },
      { elapsed: 30, publishedTime: publication },
    ],
  });
  assert.ok(Number.isFinite((await runOrdinaryGallery(page, 100, lesson)).wallMs));
  assert.deepEqual(calls, ["run", "published endpoint", "trusted Play"]);
  assert.deepEqual(ready, [false, true, true, true]);
  const observed = await readLiveState(page);
  assert.equal(observed.controls.elapsedSeconds, "30");
  assert.equal(observed.publishedTime, publication);
});

test("looping first pass captures one unseeked terminal hold before wrap with its run identity", async () => {
  const lesson = { id: "showcase-example", duration: 8, still_intervals: [[7, 8]] };
  const { page } = preparedAudioPage({
    run: { stale: false, result: { hasAudio: false, preparedPlayback: true,
      time: 0, playing: true, durationSeconds: 8 } },
    observations: [
      { elapsed: 7.5, publishedTime: 7.5, controls: { playing: "true" } },
      { elapsed: 0.1, publishedTime: 0.1, looped: true },
    ],
  });
  const result = await runOrdinaryGallery(page, 100, lesson);
  assert.deepEqual(result.firstPassCompletion, {
    disposition: "looped", loopCount: 1, interpreterGeneration: 3, runRequestId: 17,
    context: { exampleId: "showcase-example", selectionGeneration: 2, runGeneration: 9 },
  });
  assert.equal(result.firstPassCapture.runRequestId, 17);
  assert.equal(result.firstPassCapture.interpreterGeneration, 3);
  assert.deepEqual(result.firstPassCapture.attached.publication, { session: 7, sequence: 4 });
  assert.equal(result.firstPassCapture.rendererReceipt.time, 7.5);
  assert.deepEqual(result.firstPassCapture.rendererReceipt, {
    session: 7, sequence: 5, time: 7.5, presentedAtMs: 100, objectCount: 55,
  });
  assert.match(result.firstPassCapture.pngDataUrl, /^data:image\/png;base64,/);
});

test("terminal observation starts at Run acceptance and captures before delayed duration metadata", async () => {
  const lesson = { id: "showcase-example", duration: 8, still_intervals: [[7, 8]] };
  const captureEvents = [];
  const { page, window } = preparedAudioPage({
    runDelayMs: 60,
    run: { stale: false, result: { hasAudio: false, preparedPlayback: true,
      time: 0, playing: true, durationSeconds: 8 } },
    observations: [
      { elapsed: 6.5, publishedTime: 6.5, captureEvents, controls: { playing: "true" } },
      { elapsed: 7.5, publishedTime: 7.5, captureEvents, controls: { playing: "true", durationKnown: "false" } },
      { elapsed: 0.1, publishedTime: 0.1, looped: true, captureEvents },
    ],
  });
  window.__noonExampleGallery.executionMetrics = () => new Promise(() => {});
  const result = await runOrdinaryGallery(page, 250, lesson);
  assert.ok(result.firstPassCapture.observerStartedAtEpochMs >= result.firstPassCapture.runRequestedAtEpochMs);
  assert.ok(result.firstPassCapture.capturedAtEpochMs < result.firstPassCapture.runResolvedAtEpochMs);
  assert.ok(captureEvents.indexOf("toBlob-copy") < captureEvents.indexOf("run-resolved"));
});

test("a Run rejected before generation acceptance fails directly rather than timing out", async () => {
  const { page, window } = preparedAudioPage({
    rejectBeforeGeneration: new Error("source failed before Run acceptance"),
  });
  await assert.rejects(runOrdinaryGallery(page, 100), /source failed before Run acceptance/);
  assert.equal(window.__showcaseLiveReviewRun.stop, true);
});

test("an early observer error is reported without waiting for delayed Run metadata", async () => {
  const lesson = { id: "showcase-example", duration: 8, still_intervals: [[7, 8]] };
  const { page } = preparedAudioPage({
    runDelayMs: 100,
    selectedExampleId: "different-example",
    run: { stale: false, result: { hasAudio: false, preparedPlayback: true,
      time: 0, playing: true, durationSeconds: 8 } },
    observations: [{ elapsed: 7.5, publishedTime: 7.5, controls: { playing: "true" } }],
  });
  await assert.rejects(runOrdinaryGallery(page, 250, lesson), /selection changed/);
});

test("open-prefix terminal proof uses current Rust time/frontier and rejects a known mismatched end", async () => {
  const lesson = { id: "showcase-example", duration: 8, still_intervals: [[7, 8]] };
  const valid = preparedAudioPage({
    run: { stale: false, result: { hasAudio: false, preparedPlayback: true,
      time: 0, playing: true, durationSeconds: 8 } },
    observations: [
      { elapsed: 7.5, publishedTime: 7.5, end: null, frontier: 8,
        controls: { playing: "true", durationKnown: "false" } },
      { elapsed: 0.1, publishedTime: 0.1, looped: true },
    ],
  });
  assert.ok((await runOrdinaryGallery(valid.page, 100, lesson)).firstPassCapture);
  for (const invalid of [{ end: 9 }, { frontier: 7.4 }]) {
    const failed = preparedAudioPage({
      run: { stale: false, result: { hasAudio: false, preparedPlayback: true,
        time: 0, playing: true, durationSeconds: 8 } },
      observations: [
        { elapsed: 7.5, publishedTime: 7.5, ...invalid, controls: { playing: "true", durationKnown: "false" } },
        { elapsed: 0.1, publishedTime: 0.1, looped: true },
      ],
    });
    await assert.rejects(runOrdinaryGallery(failed.page, 100, lesson), /wrapped before its terminal hold picture/);
  }
});

test("closed Rust endpoint admits a matching in-hold receipt but rejects open, looped or stale proof", async () => {
  const lesson = { id: "showcase-example", duration: 8, still_intervals: [[7, 8]] };
  const preparedRun = { stale: false, result: { hasAudio: false, preparedPlayback: true,
    time: 0, playing: true, durationSeconds: 8 } };
  const endpoint = { elapsed: 8, observedTime: 8, frontier: 8, end: 8, publishedTime: 7.25,
    controls: { playing: "true" } };
  const args = { duration: 8, holdStart: 7, exampleId: lesson.id,
    runGeneration: 9, rendererReadbackEnabled: true };
  const valid = preparedAudioPage({ run: preparedRun, observations: [endpoint] });
  await valid.page.evaluate(() => window.__noonExampleGallery.run());
  valid.window.__showcaseLiveReviewRun = { requestedAtEpochMs: 0, stop: false };
  assert.equal(await valid.page.evaluate(observeNaturalFirstPassWrap, args), false);
  assert.equal(valid.window.__showcaseLiveReviewFirstPass.captureKind, "renderer-owned");
  assert.equal(valid.window.__showcaseLiveReviewFirstPass.executionState.time, 8);
  assert.equal(valid.window.__showcaseLiveReviewFirstPass.rendererReceipt.time, 7.25);
  assert.equal(valid.window.__showcaseLiveReviewFirstPass.proofReceipt.time, 7.25);

  for (const invalid of [
    { end: null }, { end: 9 }, { rustLoopCount: 1 }, { frontier: 7.9 }, { sequence: 3 },
  ]) {
    const failed = preparedAudioPage({ run: preparedRun, observations: [{ ...endpoint, ...invalid }] });
    await failed.page.evaluate(() => window.__noonExampleGallery.run());
    failed.window.__showcaseLiveReviewRun = { requestedAtEpochMs: 0, stop: false };
    failed.window.__showcaseLiveReviewFirstPass = null;
    assert.equal(await failed.page.evaluate(observeNaturalFirstPassWrap, args), false);
    assert.equal(failed.window.__showcaseLiveReviewFirstPass, null);
  }
});

test("looping first pass accepts stale controls only with matching Rust-held screenshot proof", async () => {
  const lesson = { id: "showcase-example", duration: 8, still_intervals: [[6, 8]] };
  const staleElapsed = 5.9;
  const held = { publication: { session: 7, sequence: 5 }, evaluatedTime: 5.9514,
    time: 7.5, validUntil: 8 };
  const { page } = preparedAudioPage({
    run: { stale: false, result: { hasAudio: false, preparedPlayback: true,
      time: 0, playing: true, durationSeconds: 8 } },
    observations: [
      { elapsed: staleElapsed, frontier: 8, publishedTime: 5.9514, objectCount: 626,
        controls: { playing: "true" }, heldScreenshotResult: {
        pngDataUrl: "data:image/png;base64,cGFnZQ==", screenshotStartedAtEpochMs: 11,
        screenshotReturnedAtEpochMs: 12, screenshotBounds: { x: 0, y: 0, width: 960, height: 540 },
        before: { executionState: { time: 7.5, preparedPlayback: true, loopCount: 0, playing: true, buffering: false,
          preparedFrontier: 8, durationSeconds: 8,
          presentationReceipt: { session: 7, sequence: 5, time: 5.9514, objectCount: 626 },
          heldPresentation: { ...held, observedTime: 7.5 } }, stateRequestStartedAtEpochMs: 10 },
        after: { executionState: { time: 7.5, preparedPlayback: true, loopCount: 0, playing: true, buffering: false,
          preparedFrontier: 8, durationSeconds: 8,
          presentationReceipt: { session: 7, sequence: 5, time: 5.9514, objectCount: 626 },
          heldPresentation: { ...held, observedTime: 7.5 } }, stateRequestStartedAtEpochMs: 12 },
      }, preparedPlayback: {
        observation: { loopCount: 0, heldPresentation: held }, observedTime: 7.5 } },
      { elapsed: 0.1, publishedTime: 0.1, looped: true },
    ],
  });
  const result = await runOrdinaryGallery(page, 100, lesson, { rendererReadback: {} });
  assert.deepEqual(JSON.parse(JSON.stringify(result.firstPassCapture.heldPresentation)),
    { ...held, observedTime: 7.5 });
  assert.equal(result.firstPassCapture.controls.elapsedSeconds, String(staleElapsed),
    "capture preserves the stale controls snapshot");
  assert.equal(result.firstPassCapture.executionState.time, 7.5);
  assert.equal(result.firstPassCapture.rendererReceipt.time, 5.9514,
    "capture retains the real renderer receipt rather than relabeling its time");
  assert.equal(result.firstPassCapture.captureKind, "compositor-screenshot");
  assert.equal(result.firstPassCapture.pngDataUrl, "data:image/png;base64,cGFnZQ==");
  assert.deepEqual({ ...result.firstPassCapture.compositorScreenshot.bounds },
    { x: 0, y: 0, width: 960, height: 540 });
  assertFirstPass(lesson, { ...observed(), backend: "WebGL2" },
    "WebGL2", result.firstPassCapture);
  assert.throws(() => assertFirstPass(lesson, { ...observed(), backend: "WebGL2" }, "WebGL2", {
    ...result.firstPassCapture,
    controls: { ...result.firstPassCapture.controls, elapsedSeconds: "7.5" },
    executionState: { ...result.firstPassCapture.executionState, time: staleElapsed },
  }), /Rust observation is outside the declared terminal hold/);
  for (const invalid of [
    { ...held, publication: { session: 7, sequence: 4 } },
    { ...held, validUntil: 7.5 },
  ]) {
    const failed = preparedAudioPage({
      run: { stale: false, result: { hasAudio: false, preparedPlayback: true,
        time: 0, playing: true, durationSeconds: 8 } },
      observations: [
        { elapsed: staleElapsed, frontier: 8, publishedTime: 5.9514, objectCount: 626,
          controls: { playing: "true" }, preparedPlayback: {
          observation: { loopCount: 0, heldPresentation: invalid }, observedTime: 7.5 } },
        { elapsed: 0.1, publishedTime: 0.1, looped: true },
      ],
    });
    await assert.rejects(runOrdinaryGallery(failed.page, 100, lesson), /wrapped before its terminal hold picture/);
  }
});

test("terminal snapshot survives delayed page RAF and encoding across the loop wrap", async () => {
  const lesson = { id: "showcase-example", duration: 8, still_intervals: [[7, 8]] };
  const copiedDataUrl = "data:image/png;base64,cHJlLXdyYXA=";
  const captureEvents = [];
  const { page } = preparedAudioPage({
    run: { stale: false, result: { hasAudio: false, preparedPlayback: true,
      time: 0, playing: true, durationSeconds: 8 } },
    observations: [
      { elapsed: 7.5, publishedTime: 7.5, canvasDataUrl: copiedDataUrl, captureEvents,
        controls: { playing: "true" }, wrapDuringAnimationFrame: true, wrapDuringBlobEncoding: true },
      { elapsed: 0.1, publishedTime: 0.1, looped: true },
    ],
  });
  const result = await runOrdinaryGallery(page, 100, lesson);
  assert.equal(result.firstPassCapture.pngDataUrl, copiedDataUrl);
  assert.deepEqual({ ...result.firstPassCapture.controls }, {
    controllable: "true", busy: "false", playing: "true", durationKnown: "true",
    buffering: "false", elapsedSeconds: "7.5",
  });
  assert.deepEqual(result.firstPassCapture.rendererReceipt, {
    session: 7, sequence: 5, time: 7.5, presentedAtMs: 100, objectCount: 55,
  });
  assert.deepEqual(captureEvents, [
    "state-1", "toBlob-copy", "state-2",
    "wrap-during-encoding", "toBlob-serialize",
  ]);
});

test("loop wrap without a pre-wrap picture cannot use a post-wrap frame as first-pass evidence", async () => {
  const lesson = { id: "showcase-example", duration: 8, still_intervals: [[7, 8]] };
  const { page } = preparedAudioPage({
    run: { stale: false, result: { hasAudio: false, preparedPlayback: true,
      time: 0, playing: true, durationSeconds: 8 } },
    observations: [{ elapsed: 0.1, publishedTime: 0.1, looped: true }],
  });
  await assert.rejects(runOrdinaryGallery(page, 100, lesson), /wrapped before its terminal hold picture/);
});

test("terminal capture rejects a later source run even when its loop evidence is valid", async () => {
  const lesson = { id: "showcase-example", duration: 8, still_intervals: [[7, 8]] };
  const { page } = preparedAudioPage({
    run: { stale: false, result: { hasAudio: false, preparedPlayback: true,
      time: 0, playing: true, durationSeconds: 8 } },
    observations: [
      { elapsed: 7.5, publishedTime: 7.5, controls: { playing: "true" } },
      { elapsed: 0.1, publishedTime: 0.1, looped: true, diagnostics: {
        schemaVersion: 1, interpreterGeneration: 3, runRequestId: 18,
        context: { exampleId: "showcase-example", selectionGeneration: 2, runGeneration: 10 },
        events: [{ stage: "attached", publication: { session: 8, sequence: 1 } },
          { stage: "disposition", disposition: "looped", loopCount: 1 }],
      } },
    ],
  });
  await assert.rejects(runOrdinaryGallery(page, 100, lesson), /superseded|different source run/);
});

test("terminal capture rejects a renderer session that differs from the attached publication", async () => {
  const lesson = { id: "showcase-example", duration: 8, still_intervals: [[7, 8]] };
  const { page } = preparedAudioPage({
    run: { stale: false, result: { hasAudio: false, preparedPlayback: true,
      time: 0, playing: true, durationSeconds: 8 } },
    observations: [
      { elapsed: 7.5, publishedTime: 7.5, controls: { playing: "true" }, session: 8,
        attachedPublication: { session: 7, sequence: 4 } },
      { elapsed: 0.1, publishedTime: 0.1, looped: true, session: 8 },
    ],
  });
  await assert.rejects(runOrdinaryGallery(page, 100, lesson), /wrapped before its terminal hold picture/);
});

test("fresh pre-copy state rejects a wrapped, pending or out-of-window frame", async () => {
  const lesson = { id: "showcase-example", duration: 8, still_intervals: [[7, 8]] };
  for (const invalid of [
    { wrapOnFreshMetrics: true },
    { postPresentationSample: { session: 7, sequence: 6, time: 8.01, presentedAtMs: 110 } },
    { rustLoopCount: 1 },
    { omitPreparedPlayback: true },
    { needsPresent: true },
    { bufferedDeltas: 1 },
    { wrapSynchronouslyDuringToBlob: true },
    { advanceReceiptDuringToBlob: true },
  ]) {
    const { page } = preparedAudioPage({
      run: { stale: false, result: { hasAudio: false, preparedPlayback: true,
        time: 0, playing: true, durationSeconds: 8 } },
      observations: [
        { elapsed: 7.5, publishedTime: 7.5, controls: { playing: "true" }, ...invalid },
        { elapsed: 0.1, publishedTime: 0.1, looped: true },
      ],
    });
    await assert.rejects(runOrdinaryGallery(page, 100, lesson), /wrapped before its terminal hold picture/);
  }
});

test("dense first-pass composition is checked against the captured hold, not the post-wrap frame", () => {
  const dense = { ...entry, performance: true, still_intervals: [[7, 8]] };
  const postWrap = { ...observed(), objectCount: 0,
    controls: { ...observed().controls, elapsedSeconds: "0.1" } };
  const capture = { authoredDuration: 8, holdStart: 7, executionState: { time: 7.5 },
    controls: { elapsedSeconds: "7.5" }, rendererReceipt: { time: 7.5, objectCount: 627 } };
  assertFirstPass(dense, postWrap, "WebGPU", capture);
  assert.throws(() => assertFirstPass(dense, postWrap, "WebGPU", {
    ...capture, rendererReceipt: { ...capture.rendererReceipt, objectCount: 55 },
  }), /geometry workload/);
});

test("natural first-pass publication rejects stale-before-hold and future frames", () => {
  const lesson = { id: "terminal-hold", duration: 30, still_intervals: [[29, 30]] };
  for (const time of [29, 29.012699999999256, 30]) {
    assertNaturalFirstPassPublication(lesson, 30, time);
  }
  for (const time of [28.99, 30.01, 30 + 1e-8, undefined, null, NaN, "30"]) {
    assert.throws(() => assertNaturalFirstPassPublication(lesson, 30, time));
  }
  assert.throws(() => assertNaturalFirstPassPublication(lesson, 29, 29));
});

test("moving natural endpoints retain exact publication even inside capture rounding tolerance", () => {
  const lesson = { id: "moving-endpoint", duration: 23.4, still_intervals: [[10, 11]] };
  const actual = 23.400000000000002;
  assertNaturalFirstPassPublication(lesson, actual, actual);
  assertNaturalFirstPassPublication(lesson, actual, 23.4);
  for (const time of [11, actual - 0.001, actual - 1e-8, actual + 1e-8]) {
    assert.throws(() => assertNaturalFirstPassPublication(lesson, actual, time));
  }
});

test("ordinary Scene.construct completion does not add Play or change its first pass", async () => {
  const { page, calls } = preparedAudioPage({
    run: { stale: false, result: { hasAudio: false, time: 8, playing: false } },
  });
  assert.ok(Number.isFinite((await runOrdinaryGallery(page, 100)).wallMs));
  assert.deepEqual(calls, ["run"]);
});

test("prepared non-audio completion waits for natural playback without another Play", async () => {
  const { page, calls, ready } = preparedAudioPage({
    run: { stale: false, result: {
      hasAudio: false, preparedPlayback: true, time: 7, playing: true, durationSeconds: 8,
    } },
    observations: [
      { elapsed: 7, publishedTime: 7, controls: { playing: "true" } },
      { elapsed: 8, publishedTime: 8, controls: { playing: "true", durationKnown: "false" } },
      { elapsed: 8, publishedTime: 8, controls: { playing: "true", buffering: "true" } },
      { elapsed: 8, publishedTime: 7.999, controls: { playing: "true" } },
      { elapsed: 8, publishedTime: 8, controls: { playing: "true" } },
    ],
  });
  assert.ok(Number.isFinite((await runOrdinaryGallery(page, 100)).wallMs));
  assert.deepEqual(calls, ["run", "published endpoint"]);
  assert.deepEqual(ready, [false, false, false, true, true]);
});

test("finite audio preparation must be current, paused at zero, and have a real duration", async () => {
  for (const change of [{ time: 1 }, { playing: true }, { durationSeconds: 0 },
    { durationSeconds: NaN }, { durationSeconds: Infinity }, { durationSeconds: "8" }]) {
    const { page, calls } = preparedAudioPage({ run: {
      stale: false, result: { hasAudio: true, time: 0, playing: false, durationSeconds: 8, ...change },
    } });
    await assert.rejects(runOrdinaryGallery(page, 100), /finite audio Run/);
    assert.deepEqual(calls, ["run"]);
  }
  const { page, calls } = preparedAudioPage({ run: {
    stale: true, result: { hasAudio: true, time: 0, playing: false, durationSeconds: 8 },
  } });
  await assert.rejects(runOrdinaryGallery(page, 100), /superseded/);
  assert.deepEqual(calls, ["run"]);
});

test("finite playback errors fail and the ordinary Run deadline includes Play and endpoint publication", async () => {
  const failed = preparedAudioPage({ observations: [{ patchState: "error", patchError: "audio playback failed" }] });
  await assert.rejects(runOrdinaryGallery(failed.page, 100), /audio playback failed/);
  for (const stage of ["click", "endpoint"]) {
    const { page } = preparedAudioPage();
    if (stage === "click") page.getByRole = () => ({ click: () => new Promise(() => {}) });
    else {
      const evaluate = page.evaluate;
      page.evaluate = (predicate, argument) => argument === undefined
        ? evaluate(predicate, argument) : new Promise(() => {});
    }
    await assert.rejects(runOrdinaryGallery(page, 5), /review deadline/);
  }
});

test("first-pass errors and incomplete execution fail before replay is attempted", () => {
  for (const change of [
    { patchState: "error" }, { runInFlight: true }, { selectedExampleId: "wrong" },
    { backend: "WebGL2" }, { objectCount: 0 }, { objectCount: NaN },
    { controls: { elapsedSeconds: "0" } }, { controls: { elapsedSeconds: "7.999" } },
    { controls: { elapsedSeconds: "NaN" } },
  ]) assert.throws(() => assertFirstPass(entry, { ...observed(), ...change }, "WebGPU"));
  assert.throws(() => assertFirstPass({ ...entry, performance: true }, observed(), "WebGPU"));
});
