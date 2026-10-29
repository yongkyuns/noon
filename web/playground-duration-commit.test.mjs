import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

const main = await readFile(new URL("./main.js", import.meta.url), "utf8");

const controlsStart = main.indexOf("function updatePlaybackControls(");
const controlsEnd = main.indexOf("function ensureRuntimePreparation()", controlsStart);
assert.ok(controlsStart >= 0 && controlsEnd > controlsStart, "playback-control boundary must exist");
const controlsBody = main.slice(controlsStart, controlsEnd);
assert.match(controlsBody, /new PlaygroundPlaybackControls\([\s\S]*?\{ durationSeconds, onError: showPlaybackError \},/);

const runtimeStart = main.indexOf("async function ensureRuntimeReady(");
const runtimeEnd = main.indexOf("async function ensureExecutionReady()", runtimeStart);
assert.ok(runtimeStart >= 0 && runtimeEnd > runtimeStart, "runtime startup boundary must exist");
const runtimeBody = main.slice(runtimeStart, runtimeEnd);
assert.match(
  runtimeBody,
  /updatePlaybackControls\(\{[\s\S]*?supported: !sourceOwnsExecution && initialState\.replaySupported !== false,[\s\S]*?player: nextPlayer,[\s\S]*?durationSeconds: loopDurationSeconds,/,
  "cold playback controls must use the authored duration without publishing global scene state early",
);
assert.match(
  runtimeBody,
  /updatePlaybackControls\(\{[^}]*durationSeconds: loopDurationSeconds,[^}]*playbackState: initialState,/,
  "cold startup must deliver its complete reader observation through the duration-aware helper",
);
assert.doesNotMatch(
  runtimeBody,
  /playbackControls\??\.(?:sync|observe|setDuration)\(/,
  "startup must not bypass the helper and overwrite prepared readiness or a pending command",
);
assert.doesNotMatch(
  runtimeBody,
  /\bplaybackDurationSeconds\s*=/,
  "cold startup must not commit the global duration before runScene accepts the current result",
);

const runStart = main.indexOf("async function runScene()");
const runEnd = main.indexOf("async function selectExample(", runStart);
assert.ok(runStart >= 0 && runEnd > runStart, "runScene boundary must exist");
const runBody = main.slice(runStart, runEnd);

const beforeReconcileCheck = runBody.indexOf(
  'return recordStale(runToken, "before-reconcile");',
);
const continuationCommitCheck = runBody.indexOf(
  'return recordStale(runToken, "after-continuation-replay");',
);
const coldCommitCheck = runBody.indexOf(
  'if (!isCurrentRun(runToken)) return recordStale(runToken, "after-runtime-start");',
);
const warmCommitCheck = runBody.indexOf(
  'if (!isCurrentRun(runToken)) return recordStale(runToken, "after-reconcile");',
);
const durationCommit = runBody.indexOf("playbackDurationSeconds = authored.duration;");

assert.ok(beforeReconcileCheck >= 0, "run must reject stale authored results before execution commit");
assert.ok(continuationCommitCheck > beforeReconcileCheck, "replay adoption must have a post-transition freshness check");
assert.ok(coldCommitCheck > continuationCommitCheck, "cold startup must have a post-start freshness check");
assert.ok(warmCommitCheck > coldCommitCheck, "warm reconciliation must have a post-reconcile freshness check");
assert.ok(
  durationCommit > continuationCommitCheck && durationCommit > coldCommitCheck && durationCommit > warmCommitCheck,
  "global playback duration must commit only after the current scene has committed successfully",
);
assert.equal(
  runBody.split("playbackDurationSeconds = authored.duration;").length - 1,
  1,
  "runScene must have exactly one transactional playback-duration commit",
);

console.log("✓ playback duration commits only with the current authored scene");

// Execute the production helper. The controls double records calls only: it
// does not reimplement readiness, command arbitration, or an engine clock.
function controlsFixture() {
  const calls = [], owner = {}, pane = {}, status = { dataset: {} };
  class Controls {
    constructor(...args) { calls.push(["create", ...args]); }
    setDuration(value) { calls.push(["duration", value]); }
    setControllable(value) { calls.push(["controllable", value]); }
    setBusy(value) { calls.push(["busy", value]); }
    sync(value) { calls.push(["sync", value]); }
    observe(value) { calls.push(["observe", value]); }
  }
  const onError = () => {};
  const api = new Function("PlaygroundPlaybackControls", "status", "pane", "showPlaybackError", `
    let playbackControls = null, busyDepth = 0;
    const playbackDurationSeconds = 12345;
    const document = { querySelector(selector) {
      if (selector !== ".preview-pane") throw new Error("unexpected controls container");
      return pane;
    } };
    ${controlsBody}
    return {
      update: updatePlaybackControls,
      controls: () => playbackControls,
      setBusyDepth: value => { busyDepth = value; },
    };
  `)(Controls, status, pane, onError);
  return { ...api, calls, owner, pane, status, onError };
}

for (const existing of [false, true]) {
  for (const explicitProfile of [false, true]) {
    test(`${existing ? "warm" : "cold"} ordinary controls use this run's duration (${explicitProfile ? "explicit" : "implicit"} profile)`, () => {
      const f = controlsFixture();
      if (existing) f.update({ supported: true, player: f.owner, durationSeconds: 2 });
      const previous = f.controls();
      f.calls.length = 0;
      f.setBusyDepth(1);
      const state = Object.freeze({ time: 1.25, playing: false, durationSeconds: 99,
        canUndoTranslationDrag: true, ...(explicitProfile ? { preparedPlayback: false } : {}) });
      f.update({ supported: false, player: f.owner, durationSeconds: 7, playbackState: state });
      const expected = existing ? [["duration", 7]]
        : [["create", f.owner, f.pane, { durationSeconds: 7, onError: f.onError }]];
      expected.push(["controllable", false], ["busy", true], ["sync", { ...state, durationSeconds: 7, hasAudio: false }]);
      assert.deepEqual(f.calls, expected);
      assert.equal(f.status.dataset.playbackControls, "unavailable");
      assert.equal(state.durationSeconds, 99, "the observation must not be mutated");
      if (existing) assert.equal(f.controls(), previous, "reuse the existing controls");
    });
  }

  for (const [name, readiness] of [
    ["open", { preparedFrontier: 2, durationSeconds: null, buffering: false }],
    ["buffering", { preparedFrontier: 2, durationSeconds: null, buffering: true }],
    ["terminal", { preparedFrontier: 7, durationSeconds: 7, buffering: false }],
    ["zero-duration terminal", { preparedFrontier: 0, durationSeconds: 0, buffering: false }],
  ]) {
    test(`${existing ? "existing" : "new"} ${name} prepared controls observe readiness without a source-duration sync`, () => {
      const f = controlsFixture();
      if (existing) f.update({ supported: true, player: f.owner, durationSeconds: 2,
        playbackState: Object.freeze({ preparedPlayback: true, preparedFrontier: 1,
          durationSeconds: null, time: 0, playing: false, buffering: false }) });
      const previous = f.controls();
      f.calls.length = 0;
      const state = Object.freeze({ preparedPlayback: true, time: 0, playing: false,
        canUndoTranslationDrag: false, ...readiness });
      f.update({ supported: true, player: f.owner, durationSeconds: 99, playbackState: state });
      const expected = existing ? []
        : [["create", f.owner, f.pane, { durationSeconds: 99, onError: f.onError }]];
      expected.push(["controllable", true], ["busy", false], ["observe", state]);
      assert.deepEqual(f.calls, expected);
      assert.equal(f.calls.at(-1)[1], state, "forward the complete observation unchanged");
      assert.equal(f.status.dataset.playbackControls, "available");
      if (existing) assert.equal(f.controls(), previous, "source completion must retain the controls");
    });
  }
}

test("controls without a reader observation do not invent playback state", () => {
  const f = controlsFixture();
  f.update({ supported: false, player: f.owner, durationSeconds: 3 });
  assert.deepEqual(f.calls, [
    ["create", f.owner, f.pane, { durationSeconds: 3, onError: f.onError }],
    ["controllable", false], ["busy", false],
  ]);
});

for (const existing of [false, true]) {
  test(`${existing ? "warm" : "cold"} audio controls use the accepted audio endpoint and clear audio on replacement`, () => {
    const f = controlsFixture();
    if (existing) f.update({ supported: true, player: f.owner, durationSeconds: 2 });
    f.calls.length = 0;
    const audio = Object.freeze({ time: 1.25, playing: false, durationSeconds: 9.5,
      hasAudio: true, audioReady: true, audioMuted: false, audioVolume: 0.75,
      sections: [{ name: "Introduction", time: 0 }], canUndoTranslationDrag: false });
    f.update({ supported: true, player: f.owner, durationSeconds: 3, playbackState: audio });
    const expected = existing ? [["duration", 9.5]]
      : [["create", f.owner, f.pane, { durationSeconds: 9.5, onError: f.onError }]];
    expected.push(["controllable", true], ["busy", false], ["sync", audio]);
    assert.deepEqual(f.calls, expected);
    f.calls.length = 0;
    f.update({ supported: true, player: f.owner, durationSeconds: 4,
      playbackState: Object.freeze({ time: 0, playing: false }) });
    assert.deepEqual(f.calls.at(-1), ["sync", {
      time: 0, playing: false, durationSeconds: 4, hasAudio: false, canUndoTranslationDrag: false,
    }]);
  });
}
