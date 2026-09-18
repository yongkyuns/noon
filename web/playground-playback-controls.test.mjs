import assert from "node:assert/strict";
import test from "node:test";
import { Element, dom, flush } from "./playground-dom-fixture.mjs";
const initial = dom();
globalThis.HTMLElement = Element;
globalThis.document = initial.document;
const { PlaygroundPlaybackControls } = await import("./playground-playback-controls.js");
function fixture() {
  const { document, preview } = dom();
  globalThis.document = document;
  const calls = [];
  const player = {};
  for (const name of ["pause", "resume", "seek", "restartPlayback"]) {
    player[name] = async (time = 0) => { calls.push(name); return { time, playing: false }; };
  }
  const errors = [];
  const controls = new PlaygroundPlaybackControls(player, preview, { durationSeconds: 2, onError: (error) => errors.push(error) });
  return { controls, player, preview, calls, errors, range: preview.querySelector(".playback-scrubber"), output: preview.querySelector(".playback-time") };
}

test("source-owned progress moves despite busy/paused flags, without permitting playback commands", async () => {
  const f = fixture();
  f.controls.setControllable(false);
  f.controls.setBusy(true);
  f.controls.observe({ time: 0.4, playing: false, durationSeconds: 2 });
  assert.equal(f.range.value, "0.4");
  assert.equal(f.output.value, "0.40 s · live");
  f.controls.observe({ time: 2.5, playing: true, durationSeconds: 4 });
  assert.equal(f.range.value, "2.5"); assert.equal(f.range.max, "4");
  assert.equal(f.range.disabled, true);
  for (const selector of [".playback-toggle", ".playback-restart", ".playback-scrubber"]) {
    f.preview.querySelector(selector).dispatchEvent(new Event(selector.includes("scrubber") ? "input" : "click"));
  }
  await flush(); assert.deepEqual(f.calls, []);
});

test("completed replay observes exact time and playing state without an interpolated UI clock", () => {
  const f = fixture();
  f.controls.observe({ time: 1.2, playing: true });
  assert.equal(f.range.value, "1.2");
  f.controls.observe({ time: 2, playing: false });
  assert.equal(f.output.value, "2.00 / 2.00 s");
  assert.equal(f.preview.querySelector(".playback-toggle").textContent, "Play");
});

test("polling cannot override an in-flight user's seek", async () => {
  const f = fixture(); let resolve;
  f.player.seek = () => new Promise((done) => { resolve = done; });
  f.range.value = "1.5"; f.range.dispatchEvent(new Event("input"));
  f.controls.observe({ time: 0.2, playing: true });
  assert.equal(f.range.value, "1.5");
  resolve({ time: 1.5, playing: false }); await flush();
  assert.equal(f.output.value, "1.50 / 2.00 s");
});

test("retired command completion cannot mutate a replacement's controls or error status", async () => {
  const f = fixture(); let reject;
  f.player.pause = () => new Promise((_, fail) => { reject = fail; });
  f.preview.querySelector(".playback-toggle").dispatchEvent(new Event("click"));
  f.controls.destroy();
  const replacement = new PlaygroundPlaybackControls(f.player, f.preview, { durationSeconds: 3 });
  replacement.sync({ time: 0.75, playing: false });
  reject(new Error("old worker terminated")); await flush();
  assert.equal(replacement.element.dataset.playing, "false");
  assert.equal(replacement.element.dataset.busy, "false");
  assert.equal(f.preview.querySelector(".playback-scrubber").value, "0.75");
  assert.deepEqual(f.errors, []);
});
