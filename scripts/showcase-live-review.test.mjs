import assert from "node:assert/strict";
import test from "node:test";
import { assertLiveOutcome } from "./showcase-live-review.mjs";

const entry = { id: "showcase-example", duration: 8 };
function observed() {
  return {
    selectedExampleId: entry.id, patchState: "applied", runInFlight: false,
    backend: "WebGPU", objectCount: 3, duration: "7.999999999999999",
    controls: { controllable: "true", busy: "false", playing: "false", elapsedSeconds: "7.999999999999999" },
  };
}

test("live review accepts the real authored endpoint without rewriting it", () => {
  const state = observed();
  const before = structuredClone(state);
  assertLiveOutcome(entry, state, "WebGPU");
  assert.deepEqual(state, before);
});

test("incomplete, misrouted, wrong-backend or unavailable live playback fails", () => {
  for (const change of [
    { selectedExampleId: "different" }, { patchState: "error" }, { runInFlight: true },
    { backend: "WebGL2" }, { objectCount: 0 }, { objectCount: NaN },
    { duration: "" }, { duration: "NaN" }, { duration: "7.9" },
  ]) assert.throws(() => assertLiveOutcome(entry, { ...observed(), ...change }, "WebGPU"));
  for (const change of [
    { controllable: "false" }, { busy: "true" }, { playing: "true" },
    { elapsedSeconds: "0" }, { elapsedSeconds: "NaN" },
  ]) {
    const state = observed();
    Object.assign(state.controls, change);
    assert.throws(() => assertLiveOutcome(entry, state, "WebGPU"));
  }
});

test("normal-playback evidence cannot silently reduce the dynamic workload", () => {
  const dynamic = { ...entry, performance: true };
  assert.throws(() => assertLiveOutcome(dynamic, observed(), "WebGPU"));
  assertLiveOutcome(dynamic, { ...observed(), objectCount: 626 }, "WebGPU");
});
