import assert from "node:assert/strict";
import test from "node:test";
import { runInNewContext } from "node:vm";
import { assertFirstPass, assertReplayAvailable, assertNonreplayableShowcase, assertLiveOutcome, assertLiveEndpoint, assertLivePixels, assertNaturalFirstPassPublication, summarizeLiveReview, runOrdinaryGallery, readLiveState } from "./showcase-live-review.mjs";

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
  let calls = 0;
  const page = { evaluate: async () => { calls += 1; } };
  assert.ok(Number.isFinite((await runOrdinaryGallery(page, 100)).wallMs));
  await assert.rejects(runOrdinaryGallery({ evaluate: () => new Promise(() => {}) }, 5), /review deadline/);
  assert.equal(calls, 1);

  const state = { selectedExampleId: entry.id, controls: { controllable: "true" }, objectCount: 3 };
  let stateCalls = 0;
  assert.deepEqual(await readLiveState({ evaluate: async () => { stateCalls += 1; return state; } }), state);
  assert.equal(stateCalls, 1);
});

function preparedAudioPage({ run, observations = [] } = {}) {
  const calls = [], ready = [];
  let observationIndex = 0;
  let metricsCalls = 0;
  let current = {};
  const gallery = {
    selectedExampleId: "showcase-example",
    async run() {
      calls.push("run");
      return run ?? { stale: false, result: { hasAudio: true, time: 0, playing: false, durationSeconds: 8 } };
    },
    get runInFlight() { return current.runInFlight ?? false; },
    async executionMetrics() {
      metricsCalls += 1;
      current.captureEvents?.push(`metrics-${metricsCalls}`);
      if (current.wrapOnFreshMetrics) current.looped = true;
      const sample = current.postPresentationSample && metricsCalls === 1
        ? current.postPresentationSample
        : current.presentationSample ?? {
          session: current.session ?? 7, sequence: current.sequence ?? 5,
          time: current.publishedTime, presentedAtMs: 100,
        };
      return { metrics: { time: sample.time, objectCount: current.objectCount ?? 55,
        needsPresent: current.needsPresent ?? false, bufferedDeltas: current.bufferedDeltas ?? 0,
        latestPresentationSample: sample },
      ...(current.omitPreparedPlayback ? {} : { engineMetrics: { preparedPlayback: current.preparedPlayback ?? {
        observation: { loopCount: current.rustLoopCount ?? 0 },
      } } }) };
    },
    get executionDiagnostics() {
      const base = current.diagnostics ?? {
        schemaVersion: 1, interpreterGeneration: 3, runRequestId: 17,
        context: { exampleId: "showcase-example", selectionGeneration: 2, runGeneration: 9 },
        events: [{ stage: "attached", publication: current.attachedPublication ?? {
          session: current.session ?? 7, sequence: 4 } }],
      };
      return current.looped ? { ...base, events: [...base.events,
        { stage: "disposition", disposition: "looped", loopCount: 1 }] } : base;
    },
  };
  class TestBlob { constructor(dataUrl) { this.dataUrl = dataUrl; } }
  class TestFileReader {
    readAsDataURL(blob) {
      this.result = blob.dataUrl;
      setTimeout(() => this.onload?.(), 0);
    }
  }
  const browser = {
    window: { __noonExampleGallery: gallery },
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
          setTimeout(() => {
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
    if (argument !== undefined) {
      if (observationIndex === 0) calls.push("published endpoint");
      current = observations[observationIndex++] ?? {};
    }
    const value = await runInNewContext(`(${fn.toString()})(argument)`, {
      ...browser,
      argument,
      Blob: TestBlob,
      FileReader: TestFileReader,
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
    });
    if (argument !== undefined) ready.push(value);
    return value;
  };
  return {
    calls, ready,
    page: {
      evaluate,
      getByRole(role, options) {
        assert.equal(role, "button");
        assert.deepEqual(options, { name: "Play animation", exact: true });
        return { async click() { calls.push("trusted Play"); } };
      },
      async waitForTimeout(milliseconds) {
        assert.equal(milliseconds, 25);
        if (observationIndex >= observations.length) throw new Error("no completed published endpoint");
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
    run: { stale: false, result: { hasAudio: true, time: 0, playing: false, durationSeconds: 30 } },
    observations: [
      { elapsed: 29, publishedTime: publication },
      { elapsed: 30, publishedTime: 28.99 },
      { elapsed: 30, publishedTime: 30.01 },
      { elapsed: 30, publishedTime: publication },
    ],
  });
  assert.ok(Number.isFinite((await runOrdinaryGallery(page, 100, lesson)).wallMs));
  assert.deepEqual(calls, ["run", "trusted Play", "published endpoint"]);
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
  assert.equal(result.firstPassCapture.metrics.time, 7.5);
  assert.deepEqual(result.firstPassCapture.rendererReceipt, {
    session: 7, sequence: 5, time: 7.5, presentedAtMs: 100,
  });
  assert.match(result.firstPassCapture.pngDataUrl, /^data:image\/png;base64,/);
});

test("looping first pass accepts an earlier receipt only with matching Rust hold evidence", async () => {
  const lesson = { id: "showcase-example", duration: 8, still_intervals: [[6, 8]] };
  const held = { publication: { session: 7, sequence: 5 }, evaluatedTime: 5.9514,
    time: 7.5, validUntil: 8 };
  const { page } = preparedAudioPage({
    run: { stale: false, result: { hasAudio: false, preparedPlayback: true,
      time: 0, playing: true, durationSeconds: 8 } },
    observations: [
      { elapsed: 7.5, publishedTime: 5.9514, controls: { playing: "true" }, preparedPlayback: {
        observation: { loopCount: 0, heldPresentation: held }, observedTime: 7.5 } },
      { elapsed: 0.1, publishedTime: 0.1, looped: true },
    ],
  });
  const result = await runOrdinaryGallery(page, 100, lesson);
  assert.deepEqual(JSON.parse(JSON.stringify(result.firstPassCapture.heldPresentation)),
    { ...held, observedTime: 7.5 });
  assert.equal(result.firstPassCapture.rendererReceipt.time, 5.9514,
    "capture retains the real renderer receipt rather than relabeling its time");
  for (const invalid of [
    { ...held, publication: { session: 7, sequence: 4 } },
    { ...held, validUntil: 7.5 },
  ]) {
    const failed = preparedAudioPage({
      run: { stale: false, result: { hasAudio: false, preparedPlayback: true,
        time: 0, playing: true, durationSeconds: 8 } },
      observations: [
        { elapsed: 7.5, publishedTime: 5.9514, controls: { playing: "true" }, preparedPlayback: {
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
        controls: { playing: "true" }, wrapDuringAnimationFrame: true, wrapSynchronouslyDuringToBlob: true },
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
    session: 7, sequence: 5, time: 7.5, presentedAtMs: 100,
  });
  assert.deepEqual(captureEvents, [
    "metrics-1", "toBlob-copy",
    "wrap-during-toBlob", "toBlob-serialize",
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
  await assert.rejects(runOrdinaryGallery(page, 100, lesson), /different source run/);
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

test("fresh pre-copy metrics reject a wrapped or out-of-window frame", async () => {
  const lesson = { id: "showcase-example", duration: 8, still_intervals: [[7, 8]] };
  for (const invalid of [
    { wrapOnFreshMetrics: true },
    { postPresentationSample: { session: 7, sequence: 6, time: 8.01, presentedAtMs: 110 } },
    { rustLoopCount: 1 },
    { omitPreparedPlayback: true },
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
  const capture = { authoredDuration: 8, holdStart: 7,
    controls: { elapsedSeconds: "7.5" }, metrics: { time: 7.5, objectCount: 627 } };
  assertFirstPass(dense, postWrap, "WebGPU", capture);
  assert.throws(() => assertFirstPass(dense, postWrap, "WebGPU", {
    ...capture, metrics: { ...capture.metrics, objectCount: 55 },
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
