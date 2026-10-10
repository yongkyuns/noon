import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

const workflow = await readFile(new URL("../workflows/playground-product-gate.yml", import.meta.url), "utf8");
const names = ["build", "build-baseline", "build-anchor", "build-candidate",
  "measure-adjacent", "measure-cumulative", "compare"];
const boundaries = [...workflow.matchAll(/^  ([a-z][a-z-]*):\s*$/gm)]
  .filter(match => names.includes(match[1]));
assert.deepEqual(boundaries.map(match => match[1]), names,
  "full Product Gate builder/cohort/comparator topology changed");

function job(name) {
  const index = boundaries.findIndex(match => match[1] === name);
  assert.ok(index >= 0, "missing Product Gate job " + name);
  return workflow.slice(boundaries[index].index,
    index + 1 === boundaries.length ? undefined : boundaries[index + 1].index);
}
const producer = job("build");
const baseline = job("build-baseline");
const anchor = job("build-anchor");
const candidate = job("build-candidate");
const adjacent = job("measure-adjacent");
const cumulative = job("measure-cumulative");
const compare = job("compare");

test("producer pins the exact merge parents and resolves every lock only once", () => {
  assert.match(producer, /name: Build product comparison packages/);
  assert.match(producer, /ref: \$\{\{ github.sha \}\}/);
  assert.match(producer, /NOON_PRODUCT_HEAD_SHA: \$\{\{ github.event.pull_request.head.sha \}\}/);
  assert.match(producer, /product-artifact.mjs sources candidate/);
  assert.match(producer, /product-performance-anchor.mjs pin/);
  assert.match(producer, /product-artifact.mjs dependencies baseline anchor candidate/);
  assert.match(producer, /sha256sum baseline\/Cargo.lock anchor\/Cargo.lock candidate\/Cargo.lock/);
  assert.match(producer, /sha256sum --check product-lock-digests.txt/);
  assert.match(producer, /locks-artifact: \$\{\{ steps.locks-artifact.outputs.artifact-id \}\}/);
  for (const name of ["baseline", "anchor", "candidate"]) {
    assert.ok(producer.includes(name + "/Cargo.lock"), "unavailable lock for " + name);
  }
  for (const role of ["baseline", "anchor", "candidate", "candidate-fixture"]) {
    assert.doesNotMatch(producer, new RegExp("Build " + role + " production package"),
      "source coordinator must not build binaries serially");
  }
});

test("three distinct release builders consume exactly the pinned source and resolved locks", () => {
  const jobs = [
    ["baseline", baseline, "baseline-sha"],
    ["anchor", anchor, "anchor-sha"],
    ["candidate", candidate, "candidate-sha"],
  ];
  for (const [name, section, sha] of jobs) {
    assert.match(section, /^    needs: build$/m, name + " must be independently runnable");
    assert.match(section, /artifact-ids: \$\{\{ needs.build.outputs.locks-artifact \}\}/);
    assert.match(section, /sha256sum --check product-lock-digests.txt/);
    assert.match(section, /cargo metadata --locked --format-version 1/);
    assert.match(section, /SCCACHE_GHA_ENABLED: "false"/);
    assert.match(section, /SCCACHE_LOCAL_RW_MODE: "READ_WRITE"/);
    assert.match(section, /RUSTC_WRAPPER: sccache/);
    assert.match(section, /NOON_WASM_PROFILE: "release"/);
    assert.match(section, /NOON_WASM_SKIP_OPT: "0"/);
    assert.match(section, /NOON_PRODUCT_HEAD_SHA: \$\{\{ needs.build.outputs.head-sha \}\}/);
    assert.match(section, /NOON_PRODUCT_CANDIDATE_SHA: \$\{\{ needs.build.outputs.candidate-sha \}\}/);
    assert.match(section, /uses: actions\/upload-artifact@v7/);
    if (name !== "candidate") {
      assert.ok(section.includes("ref: ${{ needs.build.outputs." + sha + " }}"),
        "wrong source checkout for " + name);
    }
  }
  assert.match(anchor, /NOON_PRODUCT_BASE_SHA: \$\{\{ needs.build.outputs.anchor-sha \}\}/);
  assert.match(baseline, /NOON_PRODUCT_BASE_SHA: \$\{\{ needs.build.outputs.baseline-sha \}\}/);
  assert.match(candidate, /NOON_PRODUCT_BASE_SHA: \$\{\{ needs.build.outputs.baseline-sha \}\}/);
});

test("lock verification does not require sccache before the compiler is installed", () => {
  for (const [name, section] of [["baseline", baseline], ["anchor", anchor], ["candidate", candidate]]) {
    const start = section.indexOf("      - name: Validate all pinned Cargo locks and selected dependency graphs");
    const end = section.indexOf("      - name: Cache Cargo sources", start);
    assert.ok(start >= 0 && end > start, name + ": missing early lock check");
    const validation = section.slice(start, end);
    assert.match(validation, /sha256sum --check product-lock-digests\.txt/);
    assert.match(validation, /RUSTC_WRAPPER="" cargo metadata --locked --format-version 1/);
    assert.doesNotMatch(validation, /\(cd "\$noon_checkout" && cargo metadata/,
      name + ": metadata unexpectedly invokes an unavailable sccache wrapper");
    assert.match(section.slice(end), /- name: Use pinned sccache/);
  }
});

test("each producer publishes exact artifact IDs without reusing prior PR binaries", () => {
  assert.match(baseline, /baseline-artifact: \$\{\{ steps.baseline-artifact.outputs.artifact-id \}\}/);
  assert.match(anchor, /anchor-artifact: \$\{\{ steps.anchor-artifact.outputs.artifact-id \}\}/);
  assert.match(candidate, /candidate-artifact: \$\{\{ steps.candidate-artifact.outputs.artifact-id \}\}/);
  assert.match(candidate, /candidate-fixture-artifact: \$\{\{ steps.candidate-fixture-artifact.outputs.artifact-id \}\}/);
  assert.match(baseline, /prepare baseline baseline[\s\S]*?stamp baseline/);
  assert.match(anchor, /prepare baseline anchor[\s\S]*?stamp baseline/);
  assert.match(candidate, /prepare candidate-fixture candidate[\s\S]*?stamp candidate-fixture/);
  assert.match(candidate, /prepare candidate candidate[\s\S]*?stamp candidate/);
  for (const [section, file] of [[adjacent, "baseline-artifact"],
    [adjacent, "candidate-fixture-artifact"], [cumulative, "anchor-artifact"]]) {
    const role = file.split("-artifact")[0];
    const expectedProducer = role === "baseline" ? "build-baseline"
      : role === "anchor" ? "build-anchor" : "build-candidate";
    assert.ok(section.includes("needs." + expectedProducer + ".outputs." + file),
      "consumer must pin exact same-run " + file);
  }
  assert.doesNotMatch(workflow, /actions\/cache\/restore@[^\n]*[\s\S]{0,300}web\/pkg/,
    "release executables must not come from an untrusted prior run");
});

test("both measurement cohorts and the required comparator fail closed on unavailable builders", () => {
  for (const section of [adjacent, cumulative]) {
    assert.match(section, /^    needs: \[build, build-baseline, build-anchor, build-candidate\]$/m);
    assert.match(section, /needs.build-baseline.result == 'success'/);
    assert.match(section, /needs.build-anchor.result == 'success'/);
    assert.match(section, /needs.build-candidate.result == 'success'/);
  }
  assert.match(compare, /needs: \[build, build-baseline, build-anchor, build-candidate, measure-adjacent, measure-cumulative\]/);
  for (const name of ["BUILD", "BASELINE", "ANCHOR", "CANDIDATE", "ADJACENT", "CUMULATIVE"]) {
    assert.match(compare, new RegExp('test "\\$NOON_PRODUCT_' + name + '_RESULT" = success'));
  }
  assert.match(compare, /name: Runtime, visual, latency, and FPS regression/);
  assert.match(compare, /^    if: \$\{\{ always\(\) \}\}$/m);
  assert.match(compare, /for noon_scope in adjacent cumulative/);
  assert.match(compare, /NOON_PRODUCT_MAX_LATENCY_RATIO: "1.25"/);
  assert.match(compare, /NOON_PRODUCT_MIN_FPS_RATIO: "0.80"/);
  assert.match(compare, /NOON_PRODUCT_MAX_VISUAL_DIFF_RATIO: "0.015"/);
});
