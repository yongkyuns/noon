import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const main = await readFile(new URL("./main.js", import.meta.url), "utf8");

const runtimeStart = main.indexOf("async function ensureRuntimeReady(");
const runtimeEnd = main.indexOf("async function ensureExecutionReady()", runtimeStart);
assert.ok(runtimeStart >= 0 && runtimeEnd > runtimeStart, "runtime startup boundary must exist");
const runtimeBody = main.slice(runtimeStart, runtimeEnd);
assert.match(
  runtimeBody,
  /new PlaygroundPlaybackControls\([\s\S]*?durationSeconds: loopDurationSeconds,/,
  "cold playback controls must use the authored duration without publishing global scene state early",
);
assert.match(
  runtimeBody,
  /playbackControls\.sync\(\{[\s\S]*?durationSeconds: loopDurationSeconds,/,
  "cold playback sync must use the startup-local authored duration",
);

const runStart = main.indexOf("async function runScene()");
const runEnd = main.indexOf("async function selectExample(", runStart);
assert.ok(runStart >= 0 && runEnd > runStart, "runScene boundary must exist");
const runBody = main.slice(runStart, runEnd);

const beforeReconcileCheck = runBody.indexOf(
  'if (!isCurrentRun(runToken)) return recordStale(runToken, "before-reconcile");',
);
const coldCommitCheck = runBody.indexOf(
  'if (!isCurrentRun(runToken)) return recordStale(runToken, "after-runtime-start");',
);
const warmCommitCheck = runBody.indexOf(
  'if (!isCurrentRun(runToken)) return recordStale(runToken, "after-reconcile");',
);
const durationCommit = runBody.indexOf("playbackDurationSeconds = authored.duration;");

assert.ok(beforeReconcileCheck >= 0, "run must reject stale authored results before execution commit");
assert.ok(coldCommitCheck > beforeReconcileCheck, "cold startup must have a post-start freshness check");
assert.ok(warmCommitCheck > coldCommitCheck, "warm reconciliation must have a post-reconcile freshness check");
assert.ok(
  durationCommit > coldCommitCheck && durationCommit > warmCommitCheck,
  "global playback duration must commit only after the current scene has committed successfully",
);
assert.equal(
  runBody.split("playbackDurationSeconds = authored.duration;").length - 1,
  1,
  "runScene must have exactly one transactional playback-duration commit",
);

console.log("✓ playback duration commits only with the current authored scene");
