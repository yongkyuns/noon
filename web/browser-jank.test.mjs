import assert from "node:assert/strict";
import test from "node:test";

import { BrowserJankMonitor, estimateUnattributedFrameMs } from "./browser-jank.js";

class FakePerformanceObserver {
  static supportedEntryTypes = ["longtask"];
  static instance = null;

  constructor(callback) {
    this.callback = callback;
    this.observed = null;
    this.disconnected = false;
    FakePerformanceObserver.instance = this;
  }

  observe(options) {
    this.observed = options;
  }

  disconnect() {
    this.disconnected = true;
  }

  emit(entries) {
    this.callback({ getEntries: () => entries });
  }
}

test("clips overlapping long tasks to the requested measurement window", () => {
  const monitor = new BrowserJankMonitor(FakePerformanceObserver);
  assert.equal(monitor.start(), true);
  assert.deepEqual(FakePerformanceObserver.instance.observed, {
    type: "longtask",
    buffered: false,
  });

  FakePerformanceObserver.instance.emit([
    { startTime: 10, duration: 55 },
    { startTime: 100, duration: 80 },
    { startTime: 190, duration: 30 },
    { startTime: 300, duration: 120 },
  ]);

  assert.deepEqual(monitor.summary(50, 200), {
    supported: true,
    count: 3,
    totalMs: 105,
    maxMs: 80,
    entries: [
      { startTime: 50, duration: 15 },
      { startTime: 100, duration: 80 },
      { startTime: 190, duration: 10 },
    ],
  });
  monitor.stop();
  assert.equal(FakePerformanceObserver.instance.disconnected, true);
});

test("ignores malformed long-task entries and rejects inverted windows", () => {
  const monitor = new BrowserJankMonitor(FakePerformanceObserver);
  monitor.start();
  FakePerformanceObserver.instance.emit([
    { startTime: Number.NaN, duration: 60 },
    { startTime: 20, duration: Number.POSITIVE_INFINITY },
    { startTime: 30, duration: 0 },
    { startTime: 40, duration: -1 },
  ]);

  assert.deepEqual(monitor.summary(), {
    supported: true,
    count: 0,
    totalMs: 0,
    maxMs: 0,
    entries: [],
  });
  assert.throws(() => monitor.summary(200, 100), /measurementEndMs/);
});

test("reports unsupported environments without failing profiling", () => {
  class UnsupportedObserver {
    static supportedEntryTypes = [];
  }
  const monitor = new BrowserJankMonitor(UnsupportedObserver);
  assert.equal(monitor.start(), false);
  assert.deepEqual(monitor.summary(), { supported: false });
});

test("estimates browser/unattributed time without producing negatives", () => {
  assert.equal(estimateUnattributedFrameMs(16.7, 2.4), 14.299999999999999);
  assert.equal(estimateUnattributedFrameMs(8, 12), 0);
  assert.equal(estimateUnattributedFrameMs(Number.NaN, 2), null);
});
