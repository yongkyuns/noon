import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";
import vm from "node:vm";
import { Element, dom, flush } from "./playground-dom-fixture.mjs";

const source = await readFile(new URL("../scripts/playground-authoring-error-recovery-smoke.mjs", import.meta.url), "utf8");
const helperStart = source.indexOf("async function startReplayForExplicitRerun(page) {");
const helperEnd = source.indexOf("\nasync function waitForInitialScene(page)", helperStart);
assert.ok(helperStart >= 0 && helperEnd > helperStart, "exercise the actual hosted replay setup helper");
const helper = source.slice(helperStart, helperEnd);
const previewHelperStart = source.indexOf("async function pauseAndRecordPreview(page,");
const previewHelperEnd = source.indexOf("\nasync function assertFrozenPreview(page,", previewHelperStart);
assert.ok(previewHelperStart >= 0 && previewHelperEnd > previewHelperStart,
  "exercise the actual preview reference setup helper");
const previewHelper = source.slice(previewHelperStart, previewHelperEnd);

const initial = dom();
globalThis.HTMLElement = Element;
globalThis.document = initial.document;
const { PlaygroundPlaybackControls } = await import("./playground-playback-controls.js");

function fixture({ time = 3, playing = false, restartPlaying = false, deferredRestart = false,
  failedCommand = null, controllable = true } = {}) {
  const { document, preview } = dom();
  globalThis.document = document;
  const calls = [];
  const errors = [];
  let completeRestart = null;
  const player = {
    async restartPlayback() {
      calls.push("restartPlayback");
      if (failedCommand === "restart") throw new Error("restart failed");
      if (deferredRestart) await new Promise(resolve => { completeRestart = resolve; });
      return { time: 0, playing: restartPlaying };
    },
    async resume() {
      calls.push("resume");
      if (failedCommand === "resume") throw new Error("resume failed");
      return { time: 0, playing: true };
    },
    async pause() { assert.fail("the setup must not pause a playing replay"); },
    async seek() { assert.fail("use the existing public restart control"); },
  };
  const controls = new PlaygroundPlaybackControls(player, preview, {
    durationSeconds: 3, onError: error => errors.push(error),
  });
  controls.sync({ time, playing });
  controls.setControllable(controllable);
  const toggle = preview.querySelector(".playback-toggle");
  // Same production control elements; the minimal facade supplies only the
  // Playwright locator/evaluation calls used by the actual hosted helper.
  const pageDocument = { querySelector: selector => selector === ".playback-controls"
    ? controls.element : preview.querySelector(selector) };
  const context = vm.createContext({ assert, document: pageDocument });
  const evaluate = fn => vm.runInContext(`(${fn.toString()})()`, context);
  const page = {
    locator(selector) {
      const element = preview.querySelector(selector);
      assert.ok(element, `unexpected selector: ${selector}`);
      return {
        isEnabled: async () => !element.disabled,
        getAttribute: async name => element.getAttribute(name),
        click: async () => {
          assert.equal(element.disabled, false, "do not dispatch disabled controls");
          element.dispatchEvent(new Event("click"));
        },
      };
    },
    async waitForFunction(predicate) {
      // No invented elapsed-time policy: flush real pending control handlers.
      for (let i = 0; i < 12; i++) {
        if (evaluate(predicate)) return;
        await flush();
        if (completeRestart !== null) {
          assert.equal(controls.element.dataset.busy, "true", "wait for the restart to settle");
          assert.deepEqual(calls, ["restartPlayback"], "resume must not race pending restart");
          const resolve = completeRestart; completeRestart = null; resolve();
        }
      }
      throw new Error("playback readiness was not established");
    },
  };
  context.page = page;
  return { calls, errors, controls, toggle, context,
    run: () => vm.runInContext(`${helper}\nstartReplayForExplicitRerun(page)`, context),
  };
}

for (const [name, time, playing] of [["finished", 3, false], ["paused", 1, false], ["playing", 1, true]]) {
  test(`hosted rerun setup explicitly starts replay from a ${name} source`, async () => {
    const f = fixture({ time, playing });
    await f.run();
    assert.deepEqual(f.calls, ["restartPlayback", "resume"]);
    assert.equal(f.toggle.getAttribute("aria-label"), "Pause animation");
    assert.equal(f.toggle.disabled, false);
    assert.equal(f.controls.element.dataset.playing, "true");
    assert.deepEqual(f.errors, []);
  });
}

test("a restart that already resumes playback must not toggle back to paused", async () => {
  const f = fixture({ restartPlaying: true });
  await f.run();
  assert.deepEqual(f.calls, ["restartPlayback"]);
  assert.equal(f.controls.element.dataset.playing, "true");
});

test("delayed restart must settle before the helper sends resume", async () => {
  const f = fixture({ deferredRestart: true });
  await f.run();
  assert.deepEqual(f.calls, ["restartPlayback", "resume"]);
  assert.deepEqual(f.errors, []);
});

test("non-replayable source is not silently accepted as an active replay", async () => {
  const f = fixture({ controllable: false });
  await assert.rejects(f.run(), /replay restart must be available/);
  assert.deepEqual(f.calls, []);
});

test("a failed resume cannot satisfy the running precondition", async () => {
  const f = fixture({ failedCommand: "resume" });
  await assert.rejects(f.run(), /playback readiness was not established/);
  assert.match(f.errors[0].message, /resume failed/);
  assert.equal(f.controls.element.dataset.playing, "false");
});

test("the old completed-source assumption is rejected by the real controls", async () => {
  const f = fixture();
  // Original CI stopped at precisely this assertion: finished != replaying.
  await assert.rejects(vm.runInContext(`(async () => {
    const replayToggle = page.locator(".playback-toggle");
    assert.equal(await replayToggle.getAttribute("aria-label"), "Pause animation");
  })()`, f.context), error => error.code === "ERR_ASSERTION" && error.actual === "Play animation");
  await f.run();
  assert.equal(f.controls.element.dataset.playing, "true");
});

function previewFixture({ duration = 1, staleSession = false, blank = false } = {}) {
  const { document, preview } = dom();
  globalThis.document = document;
  const calls = [];
  const initialTime = duration === 0 ? 0 : 0.0405;
  const state = (time, playing) => ({ time, playing, preparedPlayback: true, replaySupported: true,
    preparedFrontier: duration, durationSeconds: duration, buffering: false, preparing: false,
    seekRange: { start: 0, end: duration } });
  let renderer = { time: initialTime, presentedFrames: 12, presentedSession: 1,
    latestPresentationSample: { time: initialTime, session: 1, sequence: 11 } };
  let pendingPresentation = null;
  let waitingForPresentation = 0;
  const player = {
    async pause() { calls.push("pause"); return state(initialTime, false); },
    async seek(time) {
      calls.push(["seek", time]);
      pendingPresentation = time;
      // The accepted target alone is not evidence that the renderer displayed it.
      return state(time, false);
    },
    async resume() { assert.fail("preview capture must remain paused"); },
    async restartPlayback() { assert.fail("preview capture must not restart a looping source"); },
  };
  const controls = new PlaygroundPlaybackControls(player, preview, { durationSeconds: duration });
  controls.sync(state(initialTime, duration > 0));
  const context = vm.createContext({ assert, Event,
    document: { querySelector: selector => selector === ".playback-controls"
      ? controls.element : preview.querySelector(selector) },
    window: { __noonExampleGallery: { executionMetrics: async () => ({ metrics: renderer }) } },
    requestAnimationFrame: callback => queueMicrotask(callback),
    previewPixels: async () => {
      calls.push("pixels");
      return { different: blank ? 0 : duration === 0 || renderer.time === 0.5 ? 500 : 4 };
    },
  });
  const evaluate = (fn, ...args) => {
    context.args = args;
    return vm.runInContext(`(${fn.toString()})(...args)`, context);
  };
  const page = {
    locator(selector) {
      if (selector === "#patch-status") return { getAttribute: async () => "1" };
      const element = selector === ".playback-controls" ? controls.element : preview.querySelector(selector);
      assert.ok(element, `unexpected selector: ${selector}`);
      return {
        isEnabled: async () => !element.disabled,
        isDisabled: async () => element.disabled,
        getAttribute: async name => name === "max" ? element.max : name === "data-elapsed-seconds"
          ? element.dataset.elapsedSeconds : element.getAttribute(name),
        click: async () => element.click(),
        evaluate: async (fn, arg) => evaluate(fn, element, arg),
      };
    },
    evaluate: fn => evaluate(fn),
    async waitForFunction(predicate, arg) {
      for (let i = 0; i < 12; i++) {
        if (await evaluate(predicate, arg)) return;
        await flush();
        if (pendingPresentation !== null && ++waitingForPresentation === 2) {
          assert.equal(controls.element.dataset.elapsedSeconds, "0.5",
            "the control already advertises the accepted target while the renderer is still stale");
          assert.equal(calls.includes("pixels"), false, "readback must wait for a matching presentation");
          renderer = { ...renderer, time: pendingPresentation, presentedFrames: 13,
            latestPresentationSample: { time: pendingPresentation, session: staleSession ? 0 : 1, sequence: 12 } };
        }
      }
      throw new Error("preview presentation was not established");
    },
  };
  context.page = page;
  return { calls, run: (options = {}) => {
    context.options = options;
    return vm.runInContext(`${previewHelper}\npauseAndRecordPreview(page, options)`, context);
  } };
}

test("preview capture selects visible authored geometry instead of a loop's initial Create frame", async () => {
  const initial = previewFixture();
  await assert.rejects(initial.run(), /visible nonuniform pixels/);
  assert.deepEqual(initial.calls, ["pause", "pixels"]);

  const sampled = previewFixture();
  const reference = await sampled.run({ sampleTime: 0.5 });
  assert.deepEqual(sampled.calls, ["pause", ["seek", 0.5], "pixels"]);
  assert.equal(reference.pixels.different, 500);
  assert.equal(reference.runGeneration, "1");
});

test("an accepted preview target cannot use a stale presentation session", async () => {
  const f = previewFixture({ staleSession: true });
  await assert.rejects(f.run({ sampleTime: 0.5 }), /preview presentation was not established/);
  assert.equal(f.calls.includes("pixels"), false);
});

test("a presented but blank preview still fails the unchanged pixel guard", async () => {
  const f = previewFixture({ blank: true });
  await assert.rejects(f.run({ sampleTime: 0.5 }), /visible nonuniform pixels/);
});

test("an exact-zero preview retains disabled controls without requesting a seek", async () => {
  const f = previewFixture({ duration: 0 });
  await f.run();
  assert.deepEqual(f.calls, ["pixels"]);
});
