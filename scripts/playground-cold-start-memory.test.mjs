import assert from "node:assert/strict";
import test from "node:test";

import { aggregateProcessTreeRss, parsePsProcessTable } from "./playground-cold-start-memory.mjs";

test("process table parser converts ps RSS kilobytes to bytes", () => {
  const processes = parsePsProcessTable("  10     1  100\n  11    10  250\n");
  assert.deepEqual(processes.get(10), { ppid: 1, rssBytes: 102_400 });
  assert.deepEqual(processes.get(11), { ppid: 10, rssBytes: 256_000 });
});

test("aggregate RSS includes descendants and excludes unrelated processes", () => {
  const processes = new Map([
    [10, { ppid: 1, rssBytes: 100 }],
    [11, { ppid: 10, rssBytes: 200 }],
    [12, { ppid: 11, rssBytes: 300 }],
    [13, { ppid: 2, rssBytes: 900 }],
  ]);
  assert.deepEqual(aggregateProcessTreeRss(10, processes), { rssBytes: 600, processCount: 3 });
  assert.equal(aggregateProcessTreeRss(99, processes), null);
});

