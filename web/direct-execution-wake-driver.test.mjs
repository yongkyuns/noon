import assert from "node:assert/strict";
import test from "node:test";

import { createDirectExecutionWakeDriver } from "./direct-execution-wake-driver.js";

function fakeScheduler(initialNow = 1_000) {
  let now = initialNow;
  let nextHandle = 1;
  const animationFrames = new Map();
  const timers = new Map();

  return {
    options: {
      now: () => now,
      requestAnimationFrame(callback) {
        const handle = nextHandle++;
        animationFrames.set(handle, callback);
        return handle;
      },
      cancelAnimationFrame(handle) {
        animationFrames.delete(handle);
      },
      setTimeout(callback, delay) {
        const handle = nextHandle++;
        timers.set(handle, { callback, delay });
        return handle;
      },
      clearTimeout(handle) {
        timers.delete(handle);
      },
    },
    animationFrames,
    timers,
    setNow(value) {
      now = value;
    },
  };
}

function fakeRenderer(initialDirective, onAdvance) {
  let directive = initialDirective;
  let renders = 0;
  const advances = [];

  return {
    directWakeDirectiveJson() {
      return JSON.stringify(directive);
    },
    advanceDirectRealtime(wallTimeMs) {
      advances.push(wallTimeMs);
      directive = onAdvance?.(wallTimeMs, directive) ?? directive;
      return directive.presentNow;
    },
    render() {
      renders += 1;
      if (!directive.presentNow) {
        return false;
      }
      directive = { ...directive, presentNow: false };
      return true;
    },
    get renders() {
      return renders;
    },
    advances,
  };
}

test("static direct execution presents once and then owns no callback", () => {
  const scheduler = fakeScheduler();
  const renderer = fakeRenderer({
    presentNow: true,
    cadence: "idle",
    delayMs: null,
  });
  const driver = createDirectExecutionWakeDriver(renderer, scheduler.options);

  assert.equal(renderer.renders, 1);
  assert.equal(scheduler.animationFrames.size, 0);
  assert.equal(scheduler.timers.size, 0);
  assert.deepEqual(driver.stats(), {
    idle: true,
    scheduledAnimationFrames: 0,
    scheduledTimers: 0,
    presentationAttempts: 1,
    presentedFrames: 1,
  });
});

test("continuous direct execution uses one RAF and settles when runtime becomes idle", () => {
  const scheduler = fakeScheduler();
  const renderer = fakeRenderer(
    { presentNow: true, cadence: "animation-frame", delayMs: null },
    () => ({ presentNow: true, cadence: "idle", delayMs: null }),
  );
  const driver = createDirectExecutionWakeDriver(renderer, scheduler.options);

  assert.equal(scheduler.animationFrames.size, 1);
  const [[handle, callback]] = scheduler.animationFrames.entries();
  scheduler.animationFrames.delete(handle);
  callback(1_016);

  assert.deepEqual(renderer.advances, [1_016]);
  assert.equal(renderer.renders, 2);
  assert.equal(scheduler.animationFrames.size, 0);
  assert.equal(scheduler.timers.size, 0);
  assert.equal(driver.stats().idle, true);
  assert.equal(driver.stats().scheduledAnimationFrames, 1);
});

test("deadline direct execution uses only the delay projected by Rust", () => {
  const scheduler = fakeScheduler();
  const renderer = fakeRenderer(
    { presentNow: false, cadence: "timer", delayMs: 250 },
    () => ({ presentNow: true, cadence: "idle", delayMs: null }),
  );
  const driver = createDirectExecutionWakeDriver(renderer, scheduler.options);

  assert.equal(scheduler.animationFrames.size, 0);
  assert.equal(scheduler.timers.size, 1);
  const [[handle, timer]] = scheduler.timers.entries();
  assert.equal(timer.delay, 250);
  scheduler.timers.delete(handle);
  scheduler.setNow(1_250);
  timer.callback();

  assert.deepEqual(renderer.advances, [1_250]);
  assert.equal(renderer.renders, 1);
  assert.equal(driver.stats().idle, true);
  assert.equal(driver.stats().scheduledTimers, 1);
});
