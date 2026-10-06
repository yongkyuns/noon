// Qualification inputs/statistics only, not an engine or a runtime scheduler.
import assert from "node:assert/strict";

export const PERF_PROTOCOL = Object.freeze({ pairs: 7, warmups: 2, scoredWorkerWarmups: 1, workerLifetime: "fresh-per-pair-warmed", objects: 600,
  localEdits: 2048, segments: 32, callbacks: 4, samples: 60, sampleHz: 48,
  modes: ["async", "portable", "jspi"], workloads: ["deterministic", "segments", "callbacks"],
  maxPointRatio: 1.03, maxUpperRatio: 1.05 });

// A source fingerprint is provenance, not a compiler setting. Verify each
// artifact against its own checkout before this comparison. Cargo manifests may
// legitimately change with the code under test (e.g. adding an optional native
// crate). Compiler/tool options and out-of-manifest build recipes must still match.
export function assertComparableArtifacts(identities) {
  assert.equal(identities.length, 2, "expected baseline and candidate artifacts");
  const builds = identities.map(identity => {
    assert.equal(identity.schema, 1, "unsupported artifact schema");
    assert.match(identity.source ?? "", /^[0-9a-f]{40}$/, "missing artifact source");
    const { inputs, ...settings } = identity.build;
    assert.ok(inputs && Object.keys(inputs).length > 0, "missing source build inputs");
    for (const hash of Object.values(inputs)) {
      assert.match(hash, /^[0-9a-f]{64}$/, "invalid source build fingerprint");
    }
    const recipes = Object.fromEntries(Object.entries(inputs)
      .filter(([name]) => name !== "Cargo.toml" && !name.endsWith("/Cargo.toml")));
    return { settings, recipes };
  });
  assert.deepEqual(builds[0], builds[1], "different build configuration or recipe");
  assert.equal(typeof identities[0].compiler, "string", "missing compiler identity");
  assert.ok(identities[0].compiler.length > 0, "missing compiler identity");
  assert.equal(identities[0].compiler, identities[1].compiler, "different compiler identity");
  const names = new Set(identities.flatMap(identity => Object.keys(identity.build.inputs)));
  return [...names].sort().filter(name =>
    identities[0].build.inputs[name] !== identities[1].build.inputs[name]);
}

export function performanceSource(mode, workload) {
  assert.ok(PERF_PROTOCOL.modes.includes(mode), "unknown source mode");
  assert.ok(PERF_PROTOCOL.workloads.includes(workload), "unknown workload");
  const play = mode === "async" ? "await self.play" : mode === "jspi" ? "self._play" : "self.play";
  const wait = mode === "async" ? "await self.wait" : mode === "jspi" ? "self._wait" : "self.wait";
  const helper = mode === "jspi" ? `    def _play(self, *animations, **options):\n        self.play(*animations, **options)\n    def _wait(self, duration):\n        self.wait(duration)\n` : "";
  const work = workload === "deterministic"
    ? `        ${play}(*(dot.animate.shift(RIGHT) for dot in dots), run_time=1, rate_func=linear)`
    : workload === "segments"
      ? `        for _ in range(${PERF_PROTOCOL.segments}):\n            ${play}(dots[0].animate.shift(RIGHT / ${PERF_PROTOCOL.segments}), run_time=1 / ${PERF_PROTOCOL.segments}, rate_func=linear)`
      : `        for dot in dots[:${PERF_PROTOCOL.callbacks}]:\n            dot.add_updater(move)\n        ${wait}(1)\n        for dot in dots[:${PERF_PROTOCOL.callbacks}]:\n            dot.remove_updater(move)`;
  return `from noon import Circle, Scene, RIGHT, linear
from time import perf_counter
import inspect
import json

class HostPerformance(Scene):
${helper}    ${mode === "async" ? "async " : ""}def construct(self):
        started = perf_counter()
        dots = [Circle(0.015).shift((i % 30) * 0.03 * RIGHT) for i in range(${PERF_PROTOCOL.objects})]
        self.add(*dots)
        creation_ms = (perf_counter() - started) * 1000
        ${wait}(0)
        coroutine = bool(inspect.currentframe().f_code.co_flags & inspect.CO_COROUTINE)
        assert coroutine == ${mode === "jspi" ? "False" : "True"}, "wrong continuation mechanism"
        started = perf_counter()
        for _ in range(${PERF_PROTOCOL.localEdits}):
            dots[0].shift(0.0001 * RIGHT)
            dots[0].get_center()
        local_ms = (perf_counter() - started) * 1000
        calls = [0]
        def move(dot, dt):
            calls[0] += 1
            dot.get_center()
            dot.shift(0.1 * dt * RIGHT)
        started = perf_counter()
${work}
        execution_ms = (perf_counter() - started) * 1000
        ${wait}(0.25)
        print("NOON_PERF_REPORT " + json.dumps({"mode": "${mode}", "workload": "${workload}",
            "coroutine": coroutine, "callback_calls": calls[0], "center": list(dots[0].get_center()),
            "creation_ms": creation_ms, "local_ms": local_ms, "execution_ms": execution_ms}))
`;
}

export function pairedCost(before, after) {
  assert.equal(before.length, PERF_PROTOCOL.pairs, "missing prescribed baseline pair");
  assert.equal(after.length, before.length, "missing prescribed candidate pair");
  assert.ok([...before, ...after].every(n => Number.isFinite(n) && n > 0), "invalid timing");
  const logs = before.map((value, i) => Math.log(after[i] / value));
  const mean = logs.reduce((a, b) => a + b, 0) / logs.length;
  const variance = logs.reduce((a, b) => a + (b - mean) ** 2, 0) / (logs.length - 1);
  // Two-sided 95% Student-t interval on the seven paired log ratios, df=6.
  // Fixed before measurements. Wide/noisy intervals fail as inconclusive.
  const half = 2.446911851 * Math.sqrt(variance / logs.length);
  const ratio = Math.exp(mean), lower = Math.exp(mean - half), upper = Math.exp(mean + half);
  return { ratio, lower, upper, before, after,
    status: ratio > PERF_PROTOCOL.maxPointRatio ? "regression"
      : upper > PERF_PROTOCOL.maxUpperRatio ? "inconclusive" : "pass" };
}
