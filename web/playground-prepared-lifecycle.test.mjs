import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";
import { PlaygroundGeneration } from "./playground-generation.js";

// Execute the actual main.js lifecycle functions. Only DOM/worker endpoints
// are doubles; the browser qualification separately uses the complete page.
const source = await readFile(new URL("./main.js", import.meta.url), "utf8");
function section(begin, end) {
  const from = source.indexOf(begin), to = source.indexOf(end, from);
  assert.ok(from >= 0 && to > from, `missing lifecycle section ${begin}`);
  return source.slice(from, to);
}
const lifecycle = [
  section("function isPreparedPlayback(", "function showError("),
  section("function updatePlaybackControls(", "async function ensureExecutionReady("),
  section("async function runPlaygroundTestHook(", "async function loadDemoAuthoringSource("),
  section("function isCurrentRun(", "const runRequestRouter ="),
  section("async function updateWorkerMetrics(", "function isCurrentMetricsObservation("),
].join("\n");
const deferred = () => {
  let resolve, reject;
  const promise = new Promise((yes, no) => { resolve = yes; reject = no; });
  return { promise, resolve, reject };
};
async function until(predicate) {
  for (let i = 0; i < 100; i++) {
    if (predicate()) return;
    await Promise.resolve();
  }
  assert.fail("lifecycle did not reach the expected boundary");
}

function fixture(options = { preparedPlayback: true, preparationLookaheadSeconds: 0.75 }) {
  const calls = [], jobs = [], players = [], controls = [], errors = [];
  const dom = () => ({ disabled: false, value: "", dataset: {}, style: {} });
  const document = { visibilityState: "visible", querySelector: () => ({}) };
  const initialCanvas = dom();
  const env = {
    initialCanvas, document, calls, errors,
    generations: new PlaygroundGeneration(),
    status: dom(), patchStatus: dom(), sceneButton: dom(), sceneSourceEditor: dom(),
    resetButton: dom(), gallerySearch: dom(), categorySelect: dom(), paritySelect: dom(),
    previousGalleryPage: dom(), nextGalleryPage: dom(), metricObjects: dom(), metricTime: dom(),
    galleryGrid: { querySelectorAll: () => [] },
    previousPreview: Object.fromEntries(["hold", "showLive", "accept"].map(name =>
      [name, (...args) => calls.push([name, ...args])])),
    diagnosticPanel: { beginRun() {}, invalidate() {} },
    example: { id: "test", title: "Test", parityStatus: "candidate", inspectionZoom: true },
    hooks: { authoringRunOptions: () => options },
    preparationGate: null, stateGate: null, initialPrepared: true,
    setupGate: null, observedExecution: false,
  };
  class Controls {
    state = null;
    busy = false;
    controllable = true;
    destroyed = false;
    commandPending = false;
    constructor(player) { this.player = player; controls.push(this); }
    setDuration(duration) {
      assert.notEqual(this.state?.preparedPlayback, true, "source duration cannot overwrite reader readiness");
      this.duration = duration;
    }
    setControllable(value) { this.controllable = value; }
    setBusy(value) { this.busy = value; }
    sync(value) {
      if (this.state?.preparedPlayback) assert.equal(value.preparedPlayback, true, "do not strip prepared metadata");
      this.state = value;
    }
    observe(value) { if (!this.commandPending) this.sync(value); }
    setUnavailable() { this.controllable = false; }
    destroy() { this.destroyed = true; }
  }
  const authoring = {
    terminated: false,
    run(source, context, options) {
      const done = deferred();
      jobs.push({ source, context, options, ...done });
      return done.promise;
    },
    async cancelSemanticContinuation(contextId, generation, reason) {
      calls.push(["cancel", contextId, generation]);
      jobs.at(-1).reject(new Error(reason));
    },
    async releaseSemanticExecution(contextId) { calls.push(["release", contextId]); },
    terminate() { this.terminated = true; calls.push(["terminate-authoring"]); jobs.at(-1)?.reject(new Error("worker terminated")); },
  };
  function createRuntimeClient() {
    const runtime = {
      canvas: initialCanvas, mode: "semantic", rendererBackend: "WebGL2", terminated: false,
      options: null, status: null, stateCalls: 0,
      prepare: () => env.preparationGate?.promise ?? Promise.resolve({}),
      async startSemanticExecution(descriptor, options) {
        calls.push(["attach", descriptor]); this.options = options;
        this.status = descriptor.preparedPlayback ? {
          preparedPlayback: env.initialPrepared, time: 0.25, playing: false,
          preparedFrontier: 1, durationSeconds: null, buffering: false,
          canUndoTranslationDrag: false, replaySupported: true,
        } : { time: 0, playing: true, replaySupported: true };
        if (env.observedExecution) this.status = {
          sourceExecution: "observed", time: 0, playing: true, replaySupported: false,
          replayUnavailable: "source execution is not retained for replay", durationSeconds: null,
        };
        return { engine: { preparedPlayback: this.status.preparedPlayback }, render: { backend: "WebGL2" } };
      },
      async state() {
        this.stateCalls++;
        if (env.stateGate) await env.stateGate.promise;
        return { ...this.status };
      },
      async metrics() { return { metrics: { objectCount: 1 }, engineMetrics: { host: {} } }; },
      async pause() { calls.push(["pause"]); this.status.playing = false; return { ...this.status }; },
      async reconcileSemanticExecution(descriptor) {
        calls.push(["reconcile", descriptor]);
        assert.notEqual(this.status.preparedPlayback, true, "cannot reconcile a prepared reader");
        return { ...this.status, playing: false, time: 2 };
      },
      async retireExecutionSource() {
        calls.push(["retire-source", this]);
        this.status.playing = false;
      },
      async restart() { calls.push(["restart"]); throw new Error("unexpected receiver restart"); },
      terminate() {
        if (this.terminated) return;
        this.terminated = true; calls.push(["terminate-runtime", this]);
      },
      setInspectionZoom() { calls.push(["inspection"]); },
      async setPointerFillSelection() {},
    };
    players.push(runtime); return runtime;
  }
  const api = new Function("env", "authoring", "createRuntimeClient", "PlaygroundPlaybackControls", `
    const { calls, errors, document, generations, status, patchStatus, sceneButton, sceneSourceEditor,
      resetButton, gallerySearch, categorySelect, paritySelect, previousGalleryPage, nextGalleryPage,
      galleryGrid, previousPreview, diagnosticPanel, metricObjects, metricTime } = env;
    const globalThis = { __NOON_PLAYGROUND_TEST_HOOKS__: env.hooks };
    const console = { warn() {}, debug() {}, error() {} };
    let canvas = env.initialCanvas, player = null, preparedPlaybackPlayer = null, playbackControls = null;
    let selectedExampleId = "test", canonicalSource = "", authoringClient = authoring;
    let sceneRunPromise = null, latestRunPhaseMetrics = null, activeSourceContinuation = null;
    let activeRunRequest = null, runtimeStartPromise = null, runtimePreparation = null;
    let playerNeedsRestart = false, busyDepth = 0, galleryPage = 0, playbackDurationSeconds = 4;
    let rendererBackend = "", playbackPending = null, metricsEpoch = 0;
    const currentExample = () => env.example;
    const resetPresentationRate = () => {};
    const updateGalleryPagerControls = () => {};
    const setRuntimeStatus = (label, state) => { status.label = label; status.state = state; };
    const showSceneError = error => errors.push(error);
    const showError = showSceneError, showPlaybackError = showSceneError;
    const authoringSourceDocument = value => value;
    const ensureAuthoringClient = () => authoringClient ??= authoring;
    const adoptRuntimeCanvas = candidate => { calls.push(["adopt-canvas", candidate]); canvas = candidate.canvas; };
    const startMetricsPolling = () => calls.push(["poll"]);
    const stopMetricsPolling = () => { metricsEpoch++; calls.push(["stop-poll"]); };
    const isCurrentMetricsObservation = observation => observation !== null &&
      observation.player === player && observation.epoch === metricsEpoch &&
      observation.runGeneration === generations.diagnostics.runGeneration;
    const updateRendererMetrics = () => {};
    const ensureExecutionReady = async () => { if (playerNeedsRestart) await player.restart(); };
    const applyExampleInteraction = async (_example, target = player) => {
      target.setInspectionZoom(true); if (env.setupGate) await env.setupGate.promise;
    };
    const recordStale = (token, stage) => { generations.recordStale(token, stage); return { stale: true, stage }; };
    ${lifecycle}
    return { runScene, stopForSourceEdit, playbackPresentation, updateWorkerMetrics,
      supersedeActiveSourceContinuation,
      markFailed: () => { playerNeedsRestart = true; },
      inspect: () => ({ player, preparedPlaybackPlayer, playbackControls, busyDepth, activeSourceContinuation,
        runtimePreparation, runInFlight: sceneRunPromise !== null, latestRunPhaseMetrics }) };
  `)(env, authoring, createRuntimeClient, Controls);
  return {
    ...api, env, calls, jobs, players, controls, errors, authoring,
    async start() {
      const count = jobs.length;
      const promise = api.runScene();
      await until(() => jobs.length > count);
      return { promise, job: jobs.at(-1) };
    },
    async register(job = jobs.at(-1)) {
      job.descriptor = { contextId: `source-${jobs.indexOf(job)}`, continuationGeneration: jobs.indexOf(job) + 1,
        ...(job.options.preparedPlayback ? { preparedPlayback: true } : {}) };
      await job.options.onSemanticContinuation({ semanticExecution: job.descriptor, generation: job.descriptor.continuationGeneration, duration: 1 });
    },
    finish(job = jobs.at(-1), duration = 4) {
      const player = players.at(-1);
      if (player.status.preparedPlayback) {
        player.status.preparedFrontier = duration; player.status.durationSeconds = duration;
      }
      job.resolve({ semanticExecution: job.descriptor, duration });
    },
    count: name => calls.filter(call => call[0] === name).length,
  };
}

test("prepared startup releases only its UI busy ticket while the source remains suspended", async () => {
  const f = fixture(), { promise, job } = await f.start();
  await f.register();
  const s = f.inspect();
  assert.equal(s.runInFlight, true); assert.equal(s.busyDepth, 0);
  assert.equal(s.preparedPlaybackPlayer, s.player);
  assert.equal(s.playbackControls.controllable, true); assert.equal(s.playbackControls.busy, false);
  assert.equal(s.playbackControls.state.durationSeconds, null);
  assert.equal(s.playbackControls.state.preparedFrontier, 1);
  assert.equal(f.env.gallerySearch.disabled, false);
  assert.equal(job.options.preparationLookaheadSeconds, 0.75);
  f.finish(); await promise;
});

test("terminal adoption keeps the reader, pause, playhead and inspection without reconciliation", async () => {
  const f = fixture(), { promise } = await f.start(); await f.register();
  const before = f.inspect().player, control = f.inspect().playbackControls;
  f.finish(); const completed = await promise;
  assert.equal(completed.stale, false); assert.equal(completed.error, undefined);
  assert.equal(f.inspect().player, before); assert.equal(f.inspect().playbackControls, control);
  assert.equal(control.state.time, 0.25); assert.equal(control.state.playing, false);
  assert.equal(control.state.durationSeconds, 4);
  assert.equal(f.count("reconcile"), 0); assert.equal(f.count("pause"), 0);
  assert.equal(f.count("inspection"), 1); assert.equal(f.count("terminate-runtime"), 0);
  assert.equal(f.env.patchStatus.dataset.operation, "Prepared history retained");
  assert.deepEqual(f.errors, []);
});

test("Rust-admitted initial observed execution finishes on its original endpoint without claiming history", async () => {
  const f = fixture(); f.env.observedExecution = true;
  const { promise } = await f.start(); await f.register();
  const player = f.inspect().player;
  assert.equal(f.inspect().preparedPlaybackPlayer, null);
  assert.equal(f.inspect().playbackControls.controllable, false);
  player.status.time = 4; player.status.playing = false;
  f.finish(); const completed = await promise;
  assert.equal(completed.error, undefined); assert.equal(completed.stale, false);
  assert.equal(f.inspect().player, player);
  assert.equal(f.count("reconcile"), 0); assert.equal(f.count("terminate-runtime"), 0);
  assert.equal(f.inspect().playbackControls.controllable, false);
  assert.deepEqual(f.errors, []);
});

test("source completion cannot overwrite an in-flight user's prepared seek", async () => {
  const f = fixture(), { promise } = await f.start(); await f.register();
  const control = f.inspect().playbackControls, previous = control.state;
  control.commandPending = true; f.finish(); await promise;
  assert.equal(control.state, previous);
  control.commandPending = false;
  await f.updateWorkerMetrics(); assert.equal(control.state.durationSeconds, 4);
});

test("ordinary source errors retain open history and the next Run uses the same healthy interpreter", async () => {
  const f = fixture(), { promise, job } = await f.start(); await f.register();
  const owner = f.inspect().player, error = new Error("source exploded");
  job.reject(error); const failed = await promise;
  assert.equal(failed.retainedPreparedHistory, true); assert.equal(failed.error, error);
  assert.equal(f.inspect().player, owner); assert.equal(f.inspect().playbackControls.busy, false);
  assert.equal(f.inspect().playbackControls.state.durationSeconds, null);
  assert.equal(f.inspect().playbackControls.controllable, true);
  assert.equal(f.count("terminate-runtime"), 0); assert.deepEqual(f.errors, [error]);
  const next = await f.start(); await f.register();
  assert.notEqual(f.inspect().player, owner); assert.equal(owner.terminated, true);
  assert.equal(f.count("terminate-authoring"), 0); assert.equal(f.count("restart"), 0);
  f.finish(); await next.promise;
});

for (const reason of ["fatal interpreter", "failed endpoint"]) {
  test(`${reason} must not preserve a falsely healthy historical player`, async () => {
    const f = fixture(), { promise, job } = await f.start(); await f.register();
    if (reason === "fatal interpreter") f.authoring.terminated = true;
    else f.markFailed();
    job.reject(new Error(reason)); const failed = await promise;
    assert.equal(failed.retainedPreparedHistory, undefined);
    assert.equal(f.inspect().player, null); assert.equal(f.count("terminate-runtime"), 1);
  });
}

test("a rejected history observation after source failure retires the endpoint", async () => {
  const f = fixture(), { promise, job } = await f.start(); await f.register();
  f.inspect().player.state = async () => { throw new Error("endpoint gone"); };
  job.reject(new Error("source failed")); const failed = await promise;
  assert.equal(failed.retainedPreparedHistory, undefined); assert.equal(f.inspect().player, null);
});

test("a genuine rerun retires completed prepared history instead of attempting receiver recovery", async () => {
  const f = fixture(), first = await f.start(); await f.register(); f.finish(); await first.promise;
  const old = f.inspect().player; f.markFailed();
  const next = await f.start(); await f.register(); f.finish(); await next.promise;
  assert.equal(old.terminated, true); assert.notEqual(f.inspect().player, old);
  assert.equal(f.count("reconcile"), 0); assert.equal(f.count("restart"), 0);
});

test("editing completed prepared history retires it immediately without killing the warm interpreter", async () => {
  const f = fixture(), first = await f.start(); await f.register(); f.finish(); await first.promise;
  await f.stopForSourceEdit();
  assert.equal(f.inspect().player, null); assert.equal(f.inspect().preparedPlaybackPlayer, null);
  assert.equal(f.count("terminate-authoring"), 0); assert.equal(f.count("terminate-runtime"), 1);
});

test("editing a demand-paused source cancels its original context and suppresses stale source errors", async () => {
  const f = fixture(), { promise, job } = await f.start(); await f.register();
  await f.stopForSourceEdit(); const cancelled = await promise;
  assert.equal(cancelled.stale, true); assert.deepEqual(f.errors, []);
  assert.deepEqual(f.calls.find(call => call[0] === "cancel").slice(1),
    [job.descriptor.contextId, job.descriptor.continuationGeneration]);
  assert.equal(f.inspect().player, null); assert.equal(f.count("terminate-runtime"), 1);
  assert.equal(f.count("terminate-authoring"), 0);
});

test("stale asynchronous renderer preparation cannot attach or publish controls", async () => {
  const f = fixture(); f.env.preparationGate = deferred();
  const { promise, job } = await f.start();
  const registering = f.register().catch(error => job.reject(error));
  f.env.generations.invalidateRun(); f.env.preparationGate.resolve();
  await registering; assert.equal((await promise).stale, true);
  assert.equal(f.count("attach"), 0); assert.equal(f.controls.length, 0);
  assert.equal(f.inspect().preparedPlaybackPlayer, null);
});

test("stale asynchronous reader status cannot become the gallery's active player", async () => {
  const f = fixture(); f.env.stateGate = deferred();
  const { promise, job } = await f.start();
  const registering = f.register().catch(error => job.reject(error));
  await until(() => f.players.at(-1).stateCalls > 0);
  f.env.generations.invalidateRun(); f.env.stateGate.resolve();
  await registering; assert.equal((await promise).stale, true);
  assert.equal(f.inspect().player, null); assert.equal(f.controls.length, 0);
});

test("source-requested mode cannot override the actual attached endpoint profile", async () => {
  const f = fixture(); f.env.initialPrepared = false;
  const { promise, job } = await f.start();
  await f.register().catch(error => job.reject(error));
  const failed = await promise;
  assert.match(failed.error.message, /profile does not match/);
  assert.equal(f.inspect().player, null); assert.equal(f.controls.length, 0);
});

test("terminal metadata must match the actual adopted endpoint before gallery completion", async () => {
  const f = fixture(), { promise, job } = await f.start(); await f.register();
  job.resolve({ semanticExecution: job.descriptor, duration: 4 });
  const failed = await promise;
  assert.match(failed.error.message, /adopted terminal history/);
  assert.equal(f.inspect().player, null);
});

test("normal runs retain their existing source-owned controls and completed replay transition", async () => {
  const f = fixture(undefined); delete f.env.hooks.authoringRunOptions;
  const { promise, job } = await f.start(); await f.register();
  assert.equal(job.options.preparedPlayback, false);
  assert.equal(f.inspect().busyDepth, 1); assert.equal(f.inspect().playbackControls.controllable, false);
  f.finish(); const completed = await promise;
  assert.equal(completed.error, undefined); assert.equal(f.count("reconcile"), 1);
  assert.equal(f.inspect().busyDepth, 0); assert.equal(f.inspect().playbackControls.controllable, true);
});

test("observed pause beyond the initial segment does not claim the unfinished source is finished", () => {
  const f = fixture();
  assert.deepEqual(f.playbackPresentation({ time: 10, playing: false, sourcePlaybackControllable: true }, 1), { label: "Paused", state: "ready" });
  assert.deepEqual(f.playbackPresentation({ time: 10, playing: true, sourcePlaybackControllable: true }, 1), { label: "Playing", state: "running" });
});

for (const options of [{ preparedPlayback: "yes" }, { preparationLookaheadSeconds: 0 },
  { preparationLookaheadSeconds: NaN }]) {
  test(`invalid qualification options reject before authoring (${JSON.stringify(options)})`, async () => {
    const f = fixture(options); const failed = await f.runScene();
    assert.match(failed.error.message, /invalid prepared source run options/);
    assert.equal(f.jobs.length, 0); assert.equal(f.players.length, 0); assert.equal(f.inspect().busyDepth, 0);
  });
}

test("prepared status follows buffering, user pause and exact terminal time, not source liveness", async () => {
  const f = fixture(), { promise } = await f.start(); await f.register();
  const runtime = f.inspect().player;
  assert.equal(f.env.status.dataset.playbackPhase, "paused");
  for (const [updates, phase] of [
    [{ time: 0.5, playing: true, buffering: false }, "playing"],
    [{ playing: true, buffering: true }, "buffering"],
    [{ playing: false, buffering: true }, "paused"],
    [{ time: 3.9999, durationSeconds: 4, preparedFrontier: 4, playing: true, buffering: false }, "playing"],
    [{ time: 4 }, "finished"],
  ]) {
    Object.assign(runtime.status, updates); await f.updateWorkerMetrics();
    assert.equal(f.env.status.dataset.playbackPhase, phase);
    assert.equal(f.env.status.dataset.playbackPlaying, String(phase === "playing"));
  }
  f.finish(); await promise;
});

test("qualification can cross normal/prepared boundaries only through a fresh prepared reader", async () => {
  const f = fixture({ preparedPlayback: false });
  const first = await f.start(); await f.register(); f.finish(); await first.promise;
  const normal = f.inspect().player;
  f.env.hooks.authoringRunOptions = () => ({ preparedPlayback: true });
  const second = await f.start(); await f.register();
  assert.equal(normal.terminated, true); assert.equal(f.inspect().playbackControls.state.preparedPlayback, true);
  const prepared = f.inspect().player;
  f.finish(); await second.promise;
  f.env.hooks.authoringRunOptions = () => ({ preparedPlayback: false });
  const third = await f.start(); await f.register(); f.finish(); await third.promise;
  assert.equal(prepared.terminated, true);
  assert.equal(f.inspect().preparedPlaybackPlayer, null);
  assert.notEqual(f.inspect().playbackControls.state.preparedPlayback, true);
  assert.equal(f.count("terminate-authoring"), 0);
});

test("unsupported prepared source is not silently rerun through the normal path", async () => {
  const f = fixture(), { promise, job } = await f.start();
  job.reject(new Error("prepared source produced no readable play or wait interval"));
  const result = await promise;
  assert.match(result.error.message, /no readable/);
  assert.equal(f.jobs.length, 1); assert.equal(job.options.preparedPlayback, true);
  assert.equal(f.count("attach"), 0); assert.equal(f.count("reconcile"), 0);
});

test("a superseded partial-history observation cannot restore controls or report its old error", async () => {
  const f = fixture(), { promise, job } = await f.start(); await f.register();
  const before = f.inspect().player.stateCalls;
  f.env.stateGate = deferred(); job.reject(new Error("superseded error"));
  await until(() => f.inspect().player.stateCalls > before);
  f.env.generations.invalidateRun(); f.env.stateGate.resolve();
  assert.equal((await promise).stale, true);
  assert.equal(f.inspect().player, null); assert.deepEqual(f.errors, []);
});
