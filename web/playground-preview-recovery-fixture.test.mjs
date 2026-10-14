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
          assert.equal(toggle.disabled, true, "wait for the restart acknowledgement");
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
