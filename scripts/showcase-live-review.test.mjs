import assert from "node:assert/strict";
import test from "node:test";
import { assertLiveOutcome, assertLiveEndpoint } from "./showcase-live-review.mjs";

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

// Values observed in the failed 3343197 CI artifacts on both backends.
test("live endpoint accepts rounding above or below the decimal storyboard without relabeling", () => {
  for (const [duration, actual] of [
    [23.4, 23.400000000000002], [7.6, 7.6000000000000005],
    [2.6, 2.5999999999999996], [8, 7.999999999999999], [8, 8],
  ]) {
    assertLiveEndpoint({ duration }, actual, actual, actual);
  }
});

test("live endpoint rejects stale pixels, the wrong request and substantive duration drift", () => {
  const duration = 23.400000000000002;
  for (const args of [
    [duration, duration, duration - 0.001],
    [duration, duration, duration + 0.001],
    [duration, 23.4, duration],
    [duration + 0.001, duration + 0.001, duration + 0.001],
    [duration, duration, 21.8],
  ]) assert.throws(() => assertLiveEndpoint({ duration: 23.4 }, ...args));
});

test("live endpoint rejects missing or coerced timestamps", () => {
  for (const value of [undefined, null, NaN, Infinity, -1, 0, "8"]) {
    for (let index = 0; index < 4; index++) {
      const times = [8, 8, 8, 8]; times[index] = value;
      assert.throws(() => assertLiveEndpoint({ duration: times[0] }, ...times.slice(1)));
    }
  }
});
