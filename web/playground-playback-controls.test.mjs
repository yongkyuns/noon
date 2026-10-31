import assert from "node:assert/strict";
import test from "node:test";
import { Element, dom, flush } from "./playground-dom-fixture.mjs";
const initial = dom();
globalThis.HTMLElement = Element;
globalThis.document = initial.document;
const { PlaygroundPlaybackControls } = await import("./playground-playback-controls.js");
function fixture(withDragUndo = false) {
  const { document, preview } = dom();
  globalThis.document = document;
  const calls = [];
  const player = {};
  for (const name of ["pause", "resume", "seek", "restartPlayback"]) {
    player[name] = async (time = 0) => { calls.push(name); return { time, playing: false }; };
  }
  if (withDragUndo) player.undoTranslationDrag = async () => ({ time: 0, playing: false, canUndoTranslationDrag: false });
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

test("Undo drag is a separate visible command enabled only by the semantic receipt", async () => {
  const f = fixture(true);
  let calls = 0;
  f.player.undoTranslationDrag = async () => {
    calls += 1;
    return { time: 0, playing: false, canUndoTranslationDrag: false };
  };
  const undo = f.preview.querySelector(".playback-undo-drag");
  assert.equal(undo.hidden, false);
  assert.equal(undo.disabled, true);
  f.controls.observe({ time: 0, playing: false, canUndoTranslationDrag: true });
  assert.equal(undo.disabled, false);
  undo.dispatchEvent(new Event("click"));
  await flush();
  assert.equal(calls, 1);
  assert.equal(undo.disabled, true);
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

test("source handoff suppresses an in-flight playback rejection and permits the next seek", async () => {
  const f = fixture(); let reject;
  f.player.seek = () => new Promise((_, fail) => { reject = fail; });
  f.range.value = "1.5";
  f.range.dispatchEvent(new Event("input"));
  f.controls.setBusy(true);
  f.controls.setControllable(false);
  f.controls.setControllable(true);
  f.controls.setBusy(false);
  reject(new Error("playback controls are unavailable while a Python source continuation owns execution"));
  await flush();
  assert.deepEqual(f.errors, [], "superseded transport rejection is not a playback failure");
  f.player.seek = async (time) => ({ time, playing: false });
  f.range.value = "0.5";
  f.range.dispatchEvent(new Event("input"));
  await flush();
  assert.equal(f.output.value, "0.50 / 2.00 s");
});

test("source handoff ignores a late toggle result", async () => {
  const f = fixture(); let resolve;
  f.player.pause = () => new Promise((done) => { resolve = done; });
  f.preview.querySelector(".playback-toggle").dispatchEvent(new Event("click"));
  f.controls.setBusy(true);
  f.controls.setControllable(false);
  f.controls.sync({ time: 0.75, playing: false });
  f.controls.setControllable(true);
  f.controls.setBusy(false);
  resolve({ time: 0.25, playing: true });
  await flush();
  assert.equal(f.output.value, "0.75 / 2.00 s");
  assert.deepEqual(f.errors, []);
});

test("a current playback command rejection still reaches the error handler", async () => {
  const f = fixture();
  f.player.pause = async () => { throw new Error("current transport failure"); };
  f.preview.querySelector(".playback-toggle").dispatchEvent(new Event("click"));
  await flush();
  assert.equal(f.errors.length, 1);
  assert.match(f.errors[0].message, /current transport failure/);
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


test("silent lessons expose annotated cues and delegate navigation without resolving targets in the UI", async () => {
  const f = fixture();
  const sections = [
    { index: 0, name: "Start", time: 0 },
    { index: 1, name: "Same name", time: 0.5 },
    { index: 2, name: "Same name", time: 0.5000000001 },
  ];
  f.controls.sync({ time: 0, playing: false, sections, sectionState: { current: 0, previous: null, next: 1 } });
  assert.equal(f.preview.querySelector(".playback-audio-controls").hidden, true);
  assert.equal(f.preview.querySelector(".playback-section-controls").hidden, false);
  const menu = f.preview.querySelector(".playback-section-names");
  assert.deepEqual(menu.children.slice(1).map(option => option.value), ["0", "1", "2"]);
  assert.match(menu.children[2].textContent, /2\. Same name/);
  const markers = f.preview.querySelector(".playback-section-markers").children;
  assert.equal(markers.length, 3);
  assert.notEqual(markers[1].style.left, markers[2].style.left);
  assert.equal(f.preview.querySelector(".playback-previous-section").disabled, true);
  assert.equal(menu.value, "0");
  f.controls.sync({ time: 0.5, playing: false, sectionState: { current: 1, previous: 0, next: 2 } });
  const directions = [];
  f.player.seekSection = async previous => {
    directions.push(previous);
    return { time: 0.5, playing: false };
  };
  for (const direction of ["next", "previous"]) {
    f.preview.querySelector(`.playback-${direction}-section`).dispatchEvent(new Event("click"));
    await flush();
  }
  assert.deepEqual(directions, [false, true]);
  let selected;
  f.player.seek = async time => { selected = time; return { time, playing: true }; };
  menu.value = "2";
  menu.dispatchEvent(new Event("change"));
  await flush();
  assert.equal(selected, sections[2].time, "explicit cue jumps preserve the exact Rust timestamp");
  assert.equal(f.controls.element.dataset.playing, "true");
  assert.equal(f.preview.querySelector(".playback-section-markers").children, markers,
    "ordinary time observations do not rebuild cue annotations");
  f.controls.setControllable(false);
  assert.equal(menu.disabled, true);
  selected = null;
  menu.value = "1";
  menu.dispatchEvent(new Event("change"));
  await flush();
  assert.equal(selected, null);
  f.controls.sync({ time: 0, playing: false, sections: [] });
  assert.equal(f.preview.querySelector(".playback-section-controls").hidden, true);
});
