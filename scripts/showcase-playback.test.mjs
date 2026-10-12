import assert from "node:assert/strict";
import test from "node:test";
import { runInNewContext } from "node:vm";
import { seekPausedGallery } from "./showcase-playback.mjs";

function gallery({ playing = true, duration = 2.5999999999999996, controllable = true,
  error = null, pauseFailure = false, seekFailure = false, staleSeek = false } = {}) {
  const calls = [];
  const patch = { dataset: { state: error ? "error" : "applied" }, value: error };
  const controls = {
    dataset: { busy: "false", controllable: String(controllable), elapsedSeconds: "0" },
    title: controllable ? "" : "Replay unavailable: History not retained",
    querySelector: () => ({ getAttribute: () => playing ? "Pause animation" : "Play animation" }),
  };
  const input = {
    max: String(duration), value: "0",
    dispatchEvent() {
      calls.push("seek");
      if (seekFailure) { patch.dataset.state = "error"; patch.value = "seek rejected"; }
      else controls.dataset.elapsedSeconds = String(playing || staleSeek ? 0 : Number(input.value));
    },
  };
  const document = { querySelector: (selector) => selector === "#patch-status" ? patch : controls };
  const page = {
    async waitForFunction(fn, args) {
      const ready = runInNewContext(`(${fn})(args)`, { document, args });
      assert.equal(ready, true, "capture waited for a stable frame that cannot be reached");
      calls.push("ready");
    },
    getByRole(role, options) {
      assert.equal(role, "button"); assert.equal(options.name, "Pause animation");
      assert.equal(options.exact, true);
      return {
        count: async () => Number(playing),
        async click() {
          calls.push("pause");
          if (pauseFailure) throw new Error("pause rejected");
          playing = false;
        },
      };
    },
    locator(selector) {
      assert.equal(selector, ".playback-scrubber");
      return {
        async getAttribute(name) { assert.equal(name, "max"); return input.max; },
        async evaluate(fn, target) { return runInNewContext(`(${fn})(input, target)`, { input, target, Event: class {} }); },
      };
    },
  };
  return { page, calls, controls };
}

test("pause precedes endpoint seek and waits for stable command completion", async () => {
  const { page, calls } = gallery();
  assert.equal(await seekPausedGallery(page, 2.6), 2.5999999999999996);
  assert.deepEqual(calls, ["ready", "pause", "ready", "seek", "ready"]);
});

test("an already paused gallery is not toggled back into playback", async () => {
  const { page, calls } = gallery({ playing: false });
  await seekPausedGallery(page, 2.6);
  assert.deepEqual(calls, ["ready", "ready", "seek", "ready"]);
});

test("unavailable replay and scene errors fail explicitly rather than time out", async () => {
  await assert.rejects(seekPausedGallery(gallery({ controllable: false }).page, 2.6), /History not retained/);
  await assert.rejects(seekPausedGallery(gallery({ error: "authoring failed" }).page, 2.6), /authoring failed/);
});

test("a materially different duration is not silently substituted", async () => {
  const { page, calls } = gallery({ duration: 2.7 });
  await assert.rejects(seekPausedGallery(page, 2.6), /differs from storyboard/);
  assert.ok(!calls.includes("seek"));
});

test("pause failure prevents seeking", async () => {
  const { page, calls } = gallery({ pauseFailure: true });
  await assert.rejects(seekPausedGallery(page, 2.6), /pause rejected/);
  assert.ok(!calls.includes("seek"));
});

test("seek failure remains an error and stale endpoint observations are rejected", async () => {
  await assert.rejects(seekPausedGallery(gallery({ seekFailure: true }).page, 2.6), /seek rejected/);
  await assert.rejects(seekPausedGallery(gallery({ staleSeek: true }).page, 2.6), /stable frame/);
});

test("invalid requested durations never drive browser input", async () => {
  for (const duration of [NaN, Infinity, -1, 0]) {
    const { page, calls } = gallery();
    await assert.rejects(seekPausedGallery(page, duration));
    assert.deepEqual(calls, []);
  }
});


test("intermediate and zero checkpoints use ordinary paused UI seeks", async () => {
  const { page, calls, controls } = gallery();
  for (const time of [1.4, 0, 2.1, 0.5]) {
    assert.equal(await seekPausedGallery(page, 2.6, time), time);
    assert.equal(Number(controls.dataset.elapsedSeconds), time);
  }
  assert.equal(calls.filter(call => call === "pause").length, 1);
  assert.equal(calls.filter(call => call === "seek").length, 4);
});

test("invalid checkpoints fail before any UI command", async () => {
  for (const time of [NaN, Infinity, -1, 2.7, "1", false]) {
    const { page, calls } = gallery();
    await assert.rejects(seekPausedGallery(page, 2.6, time));
    assert.deepEqual(calls, []);
  }
});
