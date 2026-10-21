import assert from "node:assert/strict";
import test from "node:test";
import { sampleDirectProgramThrough } from "./direct-program-sample.mjs";

function quietProgram() {
  let published = 0;
  let wallTime = 0;
  let pending = false;
  const calls = [];
  return {
    calls,
    time: () => published,
    objectCount: () => published < 0.25 ? 3 : 6,
    rendererBackend: () => "WebGPU",
    lastDrawCalls: () => 1,
    seekDirect: () => assert.fail("live continuations must use their normal clock"),
    advanceDirectRealtime(time) {
      assert.equal(pending, false, "consume each publication before advancing");
      assert.ok(time >= wallTime, "wall clock must remain monotonic");
      calls.push(time);
      wallTime = time;
      const next = time >= 500 ? 0.5 : time >= 250 ? 0.25 : 0;
      if (next > published) { published = next; pending = true; }
    },
    directWakeDirectiveJson: () => JSON.stringify({
      presentNow: pending, cadence: published < 0.5 ? "timer" : "idle",
      delayMs: published < 0.5 ? (published + 0.25) * 1000 - wallTime : undefined,
    }),
    render() { assert.equal(pending, true); pending = false; return true; },
  };
}

test("live raster checkpoints retain quiet holds and cross every continuation boundary", async () => {
  const renderer = quietProgram();
  for (const [time, held, objects] of [[0.175, 0, 3], [0.3, 0.25, 6],
    [0.425, 0.25, 6], [0.5, 0.5, 6]]) {
    const sample = await sampleDirectProgramThrough(renderer, time, [0.25]);
    assert.equal(sample.time, held);
    assert.equal(sample.objectCount, objects);
    assert.equal(sample.cadence, time < 0.5 ? "timer" : "idle");
    if (time < 0.5) assert.ok(sample.delayMs > 0);
  }
  assert.deepEqual(renderer.calls, [175, 250, 300, 425, 500]);
});

test("fresh paired captures drain intermediate publications before their endpoint", async () => {
  const renderer = quietProgram();
  assert.equal((await sampleDirectProgramThrough(renderer, 0.5, [0.25, 0.25])).time, 0.5);
  assert.deepEqual(renderer.calls, [250, 500]);
});

test("live captures reject rewinds and nonfinite times before touching the host", async () => {
  const renderer = quietProgram();
  await sampleDirectProgramThrough(renderer, 0.5, [0.25]);
  const calls = [...renderer.calls];
  for (const time of [0, NaN, Infinity]) {
    await assert.rejects(sampleDirectProgramThrough(renderer, time, [0.25]), /finite monotonic/);
  }
  assert.deepEqual(renderer.calls, calls);
});
