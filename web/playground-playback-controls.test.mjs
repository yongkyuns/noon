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
  assert.equal(f.controls.element.dataset.elapsedSeconds, "0.4");
  assert.equal(f.range.hidden, true);
  assert.equal(f.output.value, "0.40 / — s");
  f.controls.observe({ time: 2.5, playing: true, durationSeconds: 4 });
  assert.equal(f.controls.element.dataset.elapsedSeconds, "2.5");
  assert.equal(f.output.value, "2.50 / — s");
  assert.equal(f.controls.durationSeconds, null, "future Python duration is not a segment horizon");
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


test("first-pass segment changes never expose a misleading percentage, and completed replay uses the full duration", () => {
  const f = fixture();
  f.controls.setControllable(false);
  for (const state of [
    { time: 0.4, durationSeconds: 0.5 },
    { time: 0.5, durationSeconds: 2.3 },
    { time: 2.7, durationSeconds: 3.05 },
    { time: 3.05, durationSeconds: 4.85 },
    { time: 5.7, durationSeconds: 5.7 },
  ]) {
    f.controls.observe({ ...state, playing: false });
    assert.equal(f.controls.element.dataset.elapsedSeconds, String(state.time));
    assert.equal(f.controls.durationSeconds, null);
    assert.equal(f.range.hidden, true);
    assert.equal(f.preview.querySelector(".playback-live-status").hidden, false);
  }
  f.controls.setDuration(5.7);
  f.controls.setControllable(true);
  f.controls.setBusy(false);
  f.controls.sync({ time: 2.7, playing: false });
  assert.equal(f.range.hidden, false);
  assert.equal(f.range.disabled, false);
  assert.equal(f.range.max, "5.7");
  assert.equal(f.range.value, "2.7");
  assert.equal(f.output.value, "2.70 / 5.70 s");
  assert.equal(f.preview.querySelector(".playback-live-status").hidden, true);
});


test("completed unavailable replay is disabled, not indefinitely busy", async () => {
  const f = fixture();
  f.controls.setBusy(true);
  f.controls.setControllable(false);
  f.controls.observe({ time: 9.2, playing: false });
  f.controls.setUnavailable("UnsupportedDomain");
  assert.equal(f.controls.element.dataset.busy, "true", "unavailability cannot clear an actual operation");
  f.controls.setBusy(false);
  assert.equal(f.controls.element.dataset.busy, "false");
  assert.equal(f.controls.element.getAttribute("aria-busy"), "false");
  assert.equal(f.controls.element.dataset.controllable, "false");
  assert.equal(f.controls.element.dataset.elapsedSeconds, "9.2");
  assert.equal(f.output.value, "9.20 s · completed");
  assert.match(f.controls.element.title, /UnsupportedDomain/);
  for (const selector of [".playback-toggle", ".playback-restart", ".playback-scrubber"]) {
    const element = f.preview.querySelector(selector);
    assert.equal(element.disabled, true);
    element.dispatchEvent(new Event(selector.includes("scrubber") ? "input" : "click"));
  }
  await flush();
  assert.deepEqual(f.calls, [], "no rejected capability may dispatch a runtime command");
});

test("unavailable capability does not hide an outstanding seek", async () => {
  const f = fixture(); let resolve;
  f.player.seek = () => new Promise(done => { resolve = done; });
  f.range.value = "1";
  f.range.dispatchEvent(new Event("input"));
  f.controls.setUnavailable("UnsupportedDomain");
  assert.equal(f.controls.element.dataset.busy, "true");
  resolve({ time: 1, playing: false });
  await flush();
  assert.equal(f.controls.element.dataset.busy, "false");
  assert.equal(f.range.disabled, true);
  assert.equal(f.controls.element.dataset.controllable, "false");
  f.controls.setControllable(true);
  assert.equal(f.range.disabled, false);
  assert.equal(f.controls.element.title, "");
});
