import assert from "node:assert/strict";
import test from "node:test";
import { Element, dom, flush } from "./playground-dom-fixture.mjs";

// Only the presentation boundary is doubled. These tests do not qualify Rust
// prefix admission, Python execution, GPU pixels or two-worker performance.
globalThis.HTMLElement = Element;
globalThis.document = dom().document;
const { PlaygroundPlaybackControls } = await import("./playground-playback-controls.js");
const prepared = (overrides = {}) => ({
  time: 0.5, playing: false, preparedPlayback: true, preparedFrontier: 2,
  durationSeconds: null, buffering: false, replaySupported: true,
  canUndoTranslationDrag: false, ...overrides,
});
function fixture(initial = prepared()) {
  const { document, preview } = dom();
  globalThis.document = document;
  let state = initial;
  const calls = [], errors = [];
  const player = {};
  for (const method of ["pause", "resume", "seek", "restartPlayback", "undoTranslationDrag"]) {
    player[method] = async (time) => {
      calls.push([method, time]);
      state = { ...state,
        time: method === "seek" ? time : method === "restartPlayback" ? 0 : state.time,
        playing: method === "pause" ? false : method === "resume" ? true : state.playing,
      };
      return state;
    };
  }
  const controls = new PlaygroundPlaybackControls(player, preview, {
    durationSeconds: 1, onError: error => errors.push(error),
  });
  controls.observe(initial);
  const element = selector => preview.querySelector(selector);
  return { document, preview, controls, player, calls, errors,
    range: element(".playback-scrubber"), input: element(".playback-prepared-seek"),
    output: element(".playback-time"), status: element(".playback-live-status"),
    toggle: element(".playback-toggle"), restart: element(".playback-restart"),
    setState(next) { state = next; controls.observe(next); },
    seek(value) { this.input.value = String(value); this.input.dispatchEvent(new Event("change")); },
  };
}
function deferred() {
  let resolve, reject;
  const promise = new Promise((yes, no) => { resolve = yes; reject = no; });
  return { promise, resolve, reject };
}

test("open history permits controls without inventing a final duration or a percentage", async () => {
  const f = fixture(prepared({ time: 3, preparedFrontier: 4 }));
  assert.equal(f.controls.durationSeconds, null);
  assert.equal(f.output.value, "3.00 / — s", "bootstrap duration must not clamp reader time");
  assert.equal(f.range.hidden, true);
  assert.equal(f.range.disabled, true);
  assert.equal(f.input.hidden, false);
  assert.equal(f.input.disabled, false);
  assert.equal(f.input.type, "number");
  assert.equal(f.input.max, "4");
  assert.match(f.status.textContent, /Prepared before 4 s · total unknown/);
  assert.equal(f.controls.element.dataset.durationKnown, "false");
  assert.equal(f.toggle.textContent, "Play");
  f.seek(2.5); await flush();
  assert.deepEqual(f.calls, [["seek", 2.5]]);
  assert.equal(f.output.value, "2.50 / — s");
});

test("buffering is not a command lock and pausing preserves the observed time", async () => {
  const f = fixture(prepared({ playing: true, buffering: true }));
  assert.equal(f.controls.element.dataset.buffering, "true");
  assert.equal(f.controls.element.dataset.busy, "false");
  assert.equal(f.controls.element.getAttribute("aria-busy"), "false");
  assert.match(f.status.textContent, /^Buffering/);
  assert.equal(f.status.getAttribute("role"), "status");
  assert.equal(f.status.getAttribute("aria-live"), "polite");
  for (const control of [f.toggle, f.restart, f.input]) assert.equal(control.disabled, false);
  f.toggle.dispatchEvent(new Event("click")); await flush();
  assert.deepEqual(f.calls, [["pause", undefined]]);
  assert.equal(f.output.value, "0.50 / — s");
  assert.equal(f.toggle.textContent, "Play");
  f.seek(0.25); await flush();
  assert.equal(f.calls.at(-1)[0], "seek");
  assert.equal(f.output.value, "0.25 / — s");
});

test("new history does not move the playhead, resume a pause or emit a command", () => {
  const f = fixture(prepared({ buffering: true }));
  f.setState(prepared({ preparedFrontier: 10 }));
  assert.equal(f.output.value, "0.50 / — s");
  assert.equal(f.controls.element.dataset.buffering, "false");
  assert.equal(f.toggle.textContent, "Play");
  assert.equal(f.input.max, "10");
  assert.deepEqual(f.calls, []);
});

test("open boundary and invalid input never send a guessed last-frame or queued future seek", async () => {
  const f = fixture();
  for (const value of [2, 3, -1, "", "NaN", "Infinity"]) {
    f.seek(value); await flush();
    assert.equal(f.input.value, "0.5");
    assert.equal(f.output.value, "0.50 / — s");
    assert.match(f.status.textContent, /before 2 s/);
  }
  assert.deepEqual(f.calls, []);
  assert.deepEqual(f.errors, []);
  f.setState(prepared({ preparedFrontier: 3 })); await flush();
  assert.deepEqual(f.calls, [], "delivery cannot replay a previously refused request");
  f.seek(2); await flush();
  assert.deepEqual(f.calls, [["seek", 2]], "a new explicit request uses the exact target");
});

test("tiny positive prefixes still admit zero without inventing an epsilon endpoint", async () => {
  const f = fixture(prepared({ time: 0, preparedFrontier: Number.MIN_VALUE }));
  f.seek(Number.MIN_VALUE); await flush();
  assert.deepEqual(f.calls, []);
  f.seek(0); await flush();
  assert.deepEqual(f.calls, [["seek", 0]]);
});

test("numeric editing survives observations and only committed edits request seeks", async () => {
  const f = fixture();
  f.document.activeElement = f.input;
  f.input.value = "1.25";
  f.input.dispatchEvent(new Event("input"));
  f.setState(prepared({ time: 0.75, preparedFrontier: 3 }));
  assert.equal(f.input.value, "1.25");
  assert.equal(f.output.value, "0.75 / — s");
  assert.deepEqual(f.calls, []);
  f.input.dispatchEvent(new Event("change")); await flush();
  assert.deepEqual(f.calls, [["seek", 1.25]]);
  assert.equal(f.output.value, "1.25 / — s");
});

test("adopted terminal metadata alone enables the full slider and exact endpoint", async () => {
  const f = fixture();
  assert.throws(() => f.controls.setDuration(8), /adopted terminal history/);
  const end = 2.5999999999999996;
  f.setState(prepared({ preparedFrontier: end, durationSeconds: end }));
  assert.equal(f.controls.durationSeconds, end);
  assert.equal(f.controls.element.dataset.durationKnown, "true");
  assert.equal(f.range.max, String(end));
  assert.equal(f.range.hidden, false);
  assert.equal(f.range.disabled, false);
  assert.equal(f.input.hidden, true);
  assert.equal(f.status.hidden, true);
  assert.deepEqual(f.calls, [], "terminal adoption does not seek or restart");
  f.range.value = "2.6"; f.range.dispatchEvent(new Event("input")); await flush();
  assert.deepEqual(f.calls, [["seek", end]]);
  assert.equal(f.output.value, "2.60 / 2.60 s");
});

test("static elapsed-time observation may equal the open boundary without authorizing that seek", async () => {
  const f = fixture(prepared({ time: 2 }));
  assert.equal(f.output.value, "2.00 / — s");
  f.seek(2); await flush();
  assert.deepEqual(f.calls, []);
  assert.equal(f.controls.durationSeconds, null);
});

test("invalid prepared observations reject atomically without changing displayed metadata", () => {
  const f = fixture();
  for (const override of [
    { preparedPlayback: false }, { replaySupported: false }, { replaySupported: undefined },
    { preparedFrontier: NaN }, { preparedFrontier: Infinity }, { preparedFrontier: 0 },
    { time: 3 }, { time: NaN }, { time: -1 }, { playing: "true" },
    { durationSeconds: undefined }, { durationSeconds: NaN }, { durationSeconds: 1 },
    { buffering: undefined }, { buffering: "false" },
    { durationSeconds: 2, buffering: true }, { canUndoTranslationDrag: "false" },
  ]) {
    assert.throws(() => f.controls.observe(prepared(override)), /state|boolean/);
    assert.equal(f.controls.durationSeconds, null);
    assert.equal(f.output.value, "0.50 / — s");
    assert.equal(f.input.max, "2");
    assert.equal(f.controls.element.dataset.buffering, "false");
    assert.equal(f.toggle.textContent, "Play");
  }
});

test("prepared controls do not accept an independent updateTime clock or lose readiness metadata", () => {
  const f = fixture(prepared({ playing: true }));
  f.controls.updateTime(1.75);
  assert.equal(f.output.value, "0.50 / — s");
  assert.throws(() => f.controls.observe({ time: 1, playing: true }), /reader frontier/);
  assert.equal(f.output.value, "0.50 / — s");
});

test("prepared profile does not expose unsupported drag undo just because the client has that method", async () => {
  const f = fixture(prepared({ canUndoTranslationDrag: true }));
  const undo = f.preview.querySelector(".playback-undo-drag");
  assert.equal(undo.hidden, true); assert.equal(undo.disabled, true);
  assert.equal(f.controls.element.dataset.undoSupport, "false");
  undo.dispatchEvent(new Event("click")); await flush();
  assert.deepEqual(f.calls, []);
});

test("failed prepared seeks restore confirmed time and surface the actual rejection", async () => {
  const f = fixture();
  f.player.seek = async () => { throw new Error("reader retired"); };
  f.seek(1); await flush();
  assert.equal(f.output.value, "0.50 / — s");
  assert.equal(f.input.value, "0.5");
  assert.equal(f.errors.length, 1);
  assert.match(f.errors[0].message, /reader retired/);
  assert.equal(f.controls.element.dataset.busy, "false");
});

test("coalesced seeks stay bounded and rejection restores the latest successful receipt", async () => {
  const f = fixture(prepared({ preparedFrontier: 4 }));
  const first = deferred(), second = deferred();
  f.player.seek = target => {
    f.calls.push(["seek", target]);
    return f.calls.length === 1 ? first.promise : second.promise;
  };
  f.seek(1); f.seek(1.5); f.seek(2);
  assert.deepEqual(f.calls, [["seek", 1]]);
  f.controls.observe(prepared({ time: 0.75 }));
  assert.equal(f.output.value, "0.50 / — s");
  first.resolve(prepared({ time: 1, preparedFrontier: 4 })); await flush();
  assert.deepEqual(f.calls, [["seek", 1], ["seek", 2]]);
  second.reject(new Error("second seek failed")); await flush();
  assert.equal(f.output.value, "1.00 / — s");
  assert.equal(f.input.value, "1");
  assert.equal(f.errors.length, 1);
});

test("completed slider also restores the latest successful receipt when a later scrub rejects", async () => {
  const f = fixture(prepared({ preparedFrontier: 4, durationSeconds: 4 }));
  const first = deferred(), second = deferred();
  f.player.seek = target => {
    f.calls.push(["seek", target]);
    return f.calls.length === 1 ? first.promise : second.promise;
  };
  f.range.value = "1"; f.range.dispatchEvent(new Event("input"));
  f.range.value = "2"; f.range.dispatchEvent(new Event("input"));
  first.resolve(prepared({ time: 1, preparedFrontier: 4, durationSeconds: 4 })); await flush();
  assert.equal(f.range.value, "2", "retain the newer optimistic slider target");
  second.reject(new Error("reader seek failed")); await flush();
  assert.equal(f.range.value, "1");
  assert.equal(f.output.value, "1.00 / 4.00 s");
});

test("busy and capability retirement still invalidate pending prepared commands", async () => {
  const f = fixture(), delayed = deferred();
  f.player.seek = () => delayed.promise;
  f.seek(1);
  f.controls.setBusy(true); f.controls.setControllable(false);
  f.controls.sync(prepared({ time: 0.25 }));
  f.controls.setControllable(true); f.controls.setBusy(false);
  delayed.reject(new Error("retired source")); await flush();
  assert.equal(f.output.value, "0.25 / — s");
  assert.deepEqual(f.errors, []);
  assert.equal(f.input.disabled, false);
});

test("destroyed prepared controls cannot mutate a replacement and remove readiness attributes", async () => {
  const f = fixture(prepared({ playing: true, buffering: true })), delayed = deferred();
  f.player.pause = () => delayed.promise;
  f.toggle.dispatchEvent(new Event("click"));
  f.controls.destroy();
  assert.equal(f.controls.element.dataset.buffering, undefined);
  assert.equal(f.controls.element.dataset.durationKnown, undefined);
  const next = new PlaygroundPlaybackControls(f.player, f.preview, { durationSeconds: 3 });
  next.sync({ time: 0.75, playing: false });
  delayed.resolve(prepared({ time: 1 })); await flush();
  assert.equal(next.durationSeconds, 3);
  assert.equal(f.preview.querySelector(".playback-time").value, "0.75 / 3.00 s");
  assert.deepEqual(f.errors, []);
});

test("explicit busy work disables prepared commands while mere source progress does not", async () => {
  const f = fixture();
  f.controls.setBusy(true);
  for (const control of [f.input, f.toggle, f.restart]) assert.equal(control.disabled, true);
  f.seek(1); f.toggle.dispatchEvent(new Event("click")); f.restart.dispatchEvent(new Event("click"));
  await flush(); assert.deepEqual(f.calls, []);
  f.controls.setBusy(false);
  f.restart.dispatchEvent(new Event("click")); await flush();
  assert.deepEqual(f.calls, [["restartPlayback", undefined]]);
  assert.equal(f.output.value, "0.00 / — s");
});
