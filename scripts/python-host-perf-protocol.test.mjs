import assert from "node:assert/strict";
import test from "node:test";
import { PERF_PROTOCOL, pairedCost, performanceSource } from "./python-host-perf-protocol.mjs";

test("cost comparison retains all pairs and rejects a regression below the old 20% FPS allowance", () => {
  const base = [100, 103, 96, 99, 105, 102, 97];
  assert.equal(pairedCost(base, base).status, "pass");
  assert.equal(pairedCost(base, base.map(n => n * 1.04)).status, "regression");
  assert.equal(pairedCost(base, base.map(n => n * 0.98)).status, "pass");
  const noisy = base.map((n, i) => n * [0.75, 1.25, 0.85, 1.1, 0.9, 1.2, 1][i]);
  assert.equal(pairedCost(base, noisy).status, "inconclusive");
  assert.throws(() => pairedCost(base.slice(1), base), /missing/);
  assert.throws(() => pairedCost(base, [NaN, ...base.slice(1)]), /invalid timing/);
  assert.throws(() => pairedCost(base, [0, ...base.slice(1)]), /invalid timing/);
});

test("all source modes use identical work, with helper calls forcing actual JSPI", () => {
  for (const work of PERF_PROTOCOL.workloads) {
    const normalize = source => source.replace(/    def _play[\s\S]*?    def construct/, "    def construct")
      .replaceAll("async def", "def").replaceAll("await self.", "self.")
      .replaceAll("self._play", "self.play").replaceAll("self._wait", "self.wait")
      .replace(/assert coroutine == (True|False)/, "assert coroutine == MODE")
      .replace(/"mode": "(async|portable|jspi)"/, '"mode": "MODE"');
    const sources = PERF_PROTOCOL.modes.map(mode => performanceSource(mode, work));
    assert.equal(normalize(sources[0]), normalize(sources[1]));
    assert.equal(normalize(sources[0]), normalize(sources[2]));
    assert.match(sources[2], /def _play/);
    assert.match(sources[2], /assert coroutine == False/);
  }
  assert.throws(() => performanceSource("unknown", "segments"));
});
