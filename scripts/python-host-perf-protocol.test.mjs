import assert from "node:assert/strict";
import test from "node:test";
import { PERF_PROTOCOL, assertComparableArtifacts, pairedCost, performanceSource } from "./python-host-perf-protocol.mjs";

test("fixed-work qualification uses independent worker observations", () => {
  assert.equal(PERF_PROTOCOL.workerLifetime, "fresh-per-observation");
  assert.equal(PERF_PROTOCOL.pairs, 7);
  assert.equal(PERF_PROTOCOL.maxPointRatio, 1.03);
  assert.equal(PERF_PROTOCOL.maxUpperRatio, 1.05);
});

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

function artifact(source) {
  return { schema: 1, source: source.repeat(40), compiler: "rustc test\nrelease: 1.98.1",
    build: { target: "wasm32-unknown-unknown", profile: "release", features: "default",
      debug: "0", incremental: "0", skipOpt: "0", toolchain: "1.98.1", binaryen: "132",
      inputs: { "Cargo.toml": "a".repeat(64), "crates/noon-web/Cargo.toml": "b".repeat(64),
        "rust-toolchain.toml": "c".repeat(64), "scripts/build-web-demo.sh": "d".repeat(64) } } };
}

test("distinct verified source inputs are not mistaken for different compiler settings", () => {
  const before = artifact("a"), after = artifact("b");
  after.build.inputs["Cargo.toml"] = "e".repeat(64);
  after.build.inputs["crates/noon-python/Cargo.toml"] = "f".repeat(64);
  assert.deepEqual(assertComparableArtifacts([before, after]),
    ["Cargo.toml", "crates/noon-python/Cargo.toml"]);
});

test("different optimization, target, feature, recipe or compiler identity still fails closed", () => {
  for (const field of ["target", "profile", "features", "debug", "incremental", "skipOpt", "toolchain", "binaryen"]) {
    const after = artifact("b"); after.build[field] = "changed";
    assert.throws(() => assertComparableArtifacts([artifact("a"), after]), /different build/);
  }
  for (const name of ["scripts/build-web-demo.sh", ".cargo/config.toml", "rust-toolchain.toml"]) {
    const after = artifact("b"); after.build.inputs[name] = "f".repeat(64);
    assert.throws(() => assertComparableArtifacts([artifact("a"), after]), /recipe/);
  }
  const after = artifact("b"); after.compiler += "\nchanged";
  assert.throws(() => assertComparableArtifacts([artifact("a"), after]), /compiler/);
});

test("missing or malformed provenance is not a comparable build", () => {
  for (const change of [a => { a.source = ""; }, a => { a.build.inputs = {}; },
    a => { a.build.inputs["Cargo.toml"] = ""; }, a => { a.compiler = ""; },
    a => { a.schema = 2; }]) {
    const before = artifact("a"); change(before);
    assert.throws(() => assertComparableArtifacts([before, artifact("b")]));
  }
  assert.throws(() => assertComparableArtifacts([artifact("a")]));
});
