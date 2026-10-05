import assert from "node:assert/strict";
import test from "node:test";
import { readFile } from "node:fs/promises";
import { productMeasurement, sampleRendererFps, samplePresentationGaps } from "./playground-product-fps.mjs";

function sample({
  now = 0,
  metricAt = now,
  rendererAt = 1_000 + now,
  frames,
  time,
  session = 1,
  clockOriginMs = 10_000,
  ...state
}) {
  return {
    now, metricAt, rendererAt, frames, time, session, clockOriginMs,
    phase: "source", ready: true, needsPresent: false,
    bufferedDeltas: 0, runInFlight: true, playbackControls: "unavailable", ...state,
  };
}

function observations({ count = 13, startFrames = 10, endFrames = 70, startTime = 0,
  endTime = 3, startAt = 2_000, endAt = 5_000, ...overrides } = {}) {
  return Array.from({ length: count }, (_, index) => {
    const progress = index / (count - 1);
    const now = index * 100;
    return sample({
      now,
      metricAt: 20 + index,
      rendererAt: startAt + progress * (endAt - startAt),
      frames: Math.round(startFrames + progress * (endFrames - startFrames)),
      time: startTime + progress * (endTime - startTime),
      ...overrides,
    });
  });
}

test("product FPS ignores a cold setup session and measures a complete authored pass", () => {
  const cold = observations({ count: 10, startFrames: 0, endFrames: 16,
    endTime: 0.9, startAt: 1_000, endAt: 1_900, session: 1 });
  const measured = observations({ count: 13, startFrames: 6, endFrames: 66,
    startAt: 2_000, endAt: 5_000, session: 2 }).map((entry, index) => ({
      ...entry, now: 1_000 + index * 100, metricAt: 40 + index,
    }));
  const fps = sampleRendererFps([...cold, ...measured], 3, { minMeasurementMs: 1_000 });
  assert.equal(fps.startFrames, 6);
  assert.equal(fps.endFrames, 66);
  assert.equal(fps.startTime, 0);
  assert.equal(fps.epochCount, 2);
  assert.equal(fps.effectiveFps, 20);
});

test("product FPS rejects a session that never completes the authored pass", () => {
  assert.throws(() => sampleRendererFps(observations({ endTime: 0.9 }), 3),
    /no settled renderer epoch covered/);
});

test("product FPS discards stale and unsettled metric replies", () => {
  const samples = [
    sample({ now: 0, metricAt: 1, rendererAt: 1_000, frames: 1, time: 0, ready: false }),
    ...observations({ count: 12, startFrames: 1, endFrames: 20, endTime: 2.75,
      startAt: 2_000, endAt: 4_750 }).map((entry, index) => ({
      ...entry, now: 100 + index * 120, metricAt: 10 + index,
    })),
    sample({ now: 200, metricAt: 10, rendererAt: 4_000, frames: 2, time: 1 }),
    sample({ now: 1_200, metricAt: 22, rendererAt: 5_000, frames: 22, time: 3,
      phase: "endpoint" }),
  ];
  const fps = sampleRendererFps(samples, 3, { minMeasurementMs: 1_000 });
  assert.equal(fps.startFrames, 1);
  assert.equal(fps.endFrames, 22);
});

test("an unsettled reset still separates renderer counters", () => {
  const first = observations({ count: 10, startFrames: 100, endFrames: 109,
    endTime: 0.9, startAt: 1_000, endAt: 1_900, session: 1 });
  const second = observations({ count: 13, startFrames: 200, endFrames: 209,
    startTime: 0, endTime: 3, startAt: 2_000, endAt: 5_000, session: 2 })
    .map((entry, index) => ({ ...entry, now: 2_000 + index * 150, metricAt: 41 + index }));
  const samples = [...first,
    sample({ now: 1_000, metricAt: 40, rendererAt: 1_950, frames: 0, time: 0,
      session: 2, ready: false }),
    ...second];
  const fps = sampleRendererFps(samples, 3);
  assert.equal(fps.epochCount, 2);
  assert.equal(fps.startFrames, 200);
  assert.equal(fps.endFrames, 209);
});

test("product FPS rejects telemetry beyond the authored endpoint", () => {
  assert.throws(() => sampleRendererFps(observations({ endTime: 3.5 }), 3),
    /no settled renderer epoch covered/);
});

test("polling delay and an idle endpoint do not extend the renderer measurement", () => {
  const base = observations({ startFrames: 12, endFrames: 72 });
  const delayedPolls = base.map((entry, index) => ({
    ...entry, now: 10_000 + index * 7_000, metricAt: 50 + index,
  }));
  const endpoint = base.at(-1);
  const duplicateIdleEndpoint = {
    ...endpoint, now: 999_999, metricAt: 99, rendererAt: 9_000,
    phase: "endpoint", runInFlight: false, playbackControls: "available",
  };
  const baselineFps = sampleRendererFps(base, 3).effectiveFps;
  const delayedFps = sampleRendererFps([...delayedPolls, duplicateIdleEndpoint], 3).effectiveFps;
  assert.equal(baselineFps, 20);
  assert.equal(delayedFps, baselineFps);
});

test("a static hold and completion handoff stay outside the scored animation window", () => {
  const samples = observations({ endTime: 4 });
  const fps = sampleRendererFps(samples, 4, { warmupSeconds: 1 });
  const withHold = sampleRendererFps([...samples,
    sample({ metricAt: 50, rendererAt: 5_100, frames: 70, time: 4 }),
    sample({ metricAt: 51, rendererAt: 5_500, frames: 71, time: 4.5 }),
    sample({ metricAt: 52, rendererAt: 5_600, frames: 72, time: 4.5,
      phase: "endpoint", session: 2 }),
  ], 4, { warmupSeconds: 1 });
  assert.equal(withHold.effectiveFps, fps.effectiveFps);
  assert.equal(withHold.endTime, 4);
  assert.equal(withHold.session, 1);
});

test("a new session cannot extend an earlier session's monotonic counter", () => {
  const first = observations({ count: 10, startFrames: 10, endFrames: 30,
    endTime: 1.5, endAt: 3_500, session: 1 });
  const second = observations({ startFrames: 31, endFrames: 110,
    startAt: 4_000, endAt: 7_000, session: 2 })
    .map((entry, index) => ({ ...entry, metricAt: 40 + index }));
  const fps = sampleRendererFps([...first, ...second], 3);
  assert.equal(fps.epochCount, 2);
  assert.equal(fps.startFrames, 31);
  assert.equal(fps.endFrames, 110);
  assert.equal(fps.measurementMs, 3_000);
});

test("a renderer clock realm change splits otherwise monotonic observations", () => {
  const first = observations({ count: 10, startFrames: 10, endFrames: 30,
    endTime: 1.5, endAt: 3_500, clockOriginMs: 10_000 });
  const second = observations({ startFrames: 31, endFrames: 60,
    startAt: 4_000, endAt: 7_000, clockOriginMs: 20_000 })
    .map((entry, index) => ({ ...entry, metricAt: 40 + index }));
  const fps = sampleRendererFps([...first, ...second], 3);
  assert.equal(fps.epochCount, 2);
  assert.equal(fps.startFrames, 31);
  assert.equal(fps.endFrames, 60);
});

test("measurement options reject invalid values", () => {
  const samples = observations();
  for (const value of [0, -1, Number.NaN, Number.POSITIVE_INFINITY]) {
    assert.throws(() => sampleRendererFps(samples, 3, { minMeasurementMs: value }),
      /minMeasurementMs|measurement duration/i);
  }
  assert.throws(() => sampleRendererFps(samples, 3, { warmupSeconds: -1 }), /warmupSeconds/i);
  assert.throws(() => sampleRendererFps(samples, 3, { maxStartDelaySeconds: -1 }),
    /maxStartDelaySeconds/i);
});

for (const [label, mutate] of [
  ["missing session", (entry) => { delete entry.session; }],
  ["invalid session", (entry) => { entry.session = -1; }],
  ["missing clock origin", (entry) => { delete entry.clockOriginMs; }],
  ["invalid clock origin", (entry) => { entry.clockOriginMs = 0; }],
  ["missing renderer time", (entry) => { delete entry.rendererAt; }],
  ["invalid renderer time", (entry) => { entry.rendererAt = Number.NaN; }],
  ["non-safe frame counter", (entry) => { entry.frames = Number.MAX_SAFE_INTEGER + 1; }],
]) {
  test(`renderer metadata ${label} rejects`, () => {
    const samples = observations();
    samples.forEach(mutate);
    assert.throws(() => sampleRendererFps(samples, 3), /renderer.*must/i);
  });
}

test("the first eligible start must fall within the bounded post-warmup window", () => {
  const samples = observations({ count: 10, startTime: 1.6, endTime: 4,
    startAt: 2_600, endAt: 5_000 });
  assert.throws(() => sampleRendererFps(samples, 4, {
    warmupSeconds: 1, maxStartDelaySeconds: 0.5,
  }), /start|warmup|covered/i);
});

test("one session and clock epoch must cover both the start window and endpoint", () => {
  const first = observations({ count: 10, startTime: 0, endTime: 1.4,
    startAt: 1_000, endAt: 2_400, session: 1 });
  const second = observations({ startTime: 1.6, endTime: 3,
    startAt: 2_500, endAt: 4_000, session: 2 })
    .map((entry, index) => ({ ...entry, metricAt: 40 + index }));
  assert.throws(() => sampleRendererFps([...first, ...second], 3, {
    warmupSeconds: 1, maxStartDelaySeconds: 0.5,
  }), /no settled renderer epoch covered/);
});

test("FPS is anchored to renderer timestamps rather than arrival and polling clocks", () => {
  const samples = observations({ startFrames: 10, endFrames: 70 });
  const baseline = sampleRendererFps(samples, 3);
  const distortedDiagnostics = samples.map((entry, index) => ({
    ...entry, now: 100_000 + index * 9_000, metricAt: 500_000 + index * 3_000,
  }));
  const changed = sampleRendererFps(distortedDiagnostics, 3);
  assert.equal(changed.measurementMs, baseline.measurementMs);
  assert.equal(changed.effectiveFps, baseline.effectiveFps);
});

test("camera measurement isolates following from setup, restoration, and endpoint holds", () => {
  const protocol = productMeasurement("showcase-camera-follows-path");
  assert.equal(protocol.windowStartSeconds, 3.7);
  assert.equal(protocol.windowEndSeconds, 6.9);
  assert.equal(protocol.sourceEndSeconds, 9.8);
  const samples = [
    ...observations({ endTime: 3.6, endAt: 4_600 }),
    ...observations({ startFrames: 100, endFrames: 292, startTime: 3.7, endTime: 6.9,
      startAt: 5_000, endAt: 8_200 }).map((entry, index) => ({ ...entry, metricAt: 100 + index })),
    sample({ metricAt: 200, rendererAt: 8_300, frames: 292, time: 6.9 }),
    sample({ metricAt: 201, rendererAt: 11_100, frames: 360, time: 9.8, phase: "endpoint" }),
  ];
  const fps = sampleRendererFps(samples, protocol.windowEndSeconds, { warmupSeconds: protocol.windowStartSeconds });
  assert.equal(fps.effectiveFps, 60);
  assert.equal(fps.startFrames, 100);
  assert.equal(fps.endFrames, 292);
  assert.equal(fps.endRendererAt, 8_200);
  assert.throws(() => productMeasurement("unqualified-scene"), /unsupported product measurement/);
});

test("the camera measurement window follows the curated source's authored beats", async () => {
  const source = await readFile(new URL("../web/python/examples/showcase_camera_follows_path.py", import.meta.url), "utf8");
  const [setup, following] = source.split("camera_frame.add_updater(follow_point)");
  assert.ok(following, "the measured camera lesson must use its required Python updater");
  const duration = section => [...section.matchAll(/run_time=([0-9.]+)|await self\.wait\(([0-9.]+)\)/g)]
    .reduce((total, match) => total + Number(match[1] ?? match[2]), 0);
  const protocol = productMeasurement("showcase-camera-follows-path");
  assert.ok(Math.abs(duration(setup) - protocol.windowStartSeconds) < 0.001);
  assert.ok(Math.abs(duration(setup) + duration(following.split("camera_frame.remove_updater")[0]) -
    protocol.windowEndSeconds) < 0.001);
  assert.ok(Math.abs(duration(source) - protocol.sourceEndSeconds) < 0.001);
});

function gapFixture() {
  const fps = sampleRendererFps(observations({ startFrames: 100, endFrames: 292,
    startTime: 3.7, endTime: 6.9, startAt: 5_000, endAt: 8_200 }), 6.9, { warmupSeconds: 3.7 });
  const samples = Array.from({ length: 193 }, (_, index) => ({ session: 1, clockOriginMs: 10_000,
    sequence: 100 + index, presentedAtMs: 4_998 + index * 1000 / 60 }));
  return { fps, samples };
}

test("publication gaps use complete worker presentation timestamps within the scored epoch", () => {
  const { fps, samples } = gapFixture();
  const gaps = samplePresentationGaps([
    { session: 2, clockOriginMs: 10_000, sequence: 0, presentedAtMs: 0 },
    ...samples,
    { session: 1, clockOriginMs: 10_000, sequence: 400, presentedAtMs: 8_300 },
  ], fps);
  assert.equal(gaps.intervalCount, fps.endFrames - fps.startFrames);
  assert.ok(Math.abs(gaps.intervalMs.mean - 1000 / 60) < 0.001);
  assert.equal(gaps.cadence.longFrameRate, 0);
});

test("high average FPS still reports jagged per-frame submission intervals", () => {
  const { fps, samples } = gapFixture();
  for (let index = 1; index < samples.length; index += 1) {
    samples[index].presentedAtMs = samples[index - 1].presentedAtMs + (index % 2 ? 2000 / 60 - 1 : 1);
  }
  const gaps = samplePresentationGaps(samples, fps);
  assert.equal(fps.effectiveFps, 60);
  assert.equal(gaps.cadence.longFrameRate, 0.5);
  assert.ok(gaps.intervalMs.p95 > 32);
});

for (const [name, mutate, expected] of [
  ["a lost frame", samples => samples.splice(20, 1), /cover every measured renderer frame/],
  ["a missing start", samples => samples.shift(), /cover every measured renderer frame/],
  ["a missing end", samples => samples.pop(), /cover every measured renderer frame/],
  ["a wrong realm", samples => { samples[20].clockOriginMs = 20_000; }, /cover every measured renderer frame/],
  ["a duplicate publication", samples => { samples[20].sequence = samples[19].sequence; }, /must advance/],
  ["a backwards clock", samples => { samples[20].presentedAtMs = samples[19].presentedAtMs - 1; }, /must advance/],
  ["a malformed timestamp", samples => { samples[20].presentedAtMs = Number.NaN; }, /must advance/],
]) {
  test(`publication gaps reject ${name}`, () => {
    const { fps, samples } = gapFixture();
    mutate(samples);
    assert.throws(() => samplePresentationGaps(samples, fps), expected);
  });
}
