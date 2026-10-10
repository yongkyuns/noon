import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

const workflow = await readFile(new URL("../workflows/playground-product-gate.yml", import.meta.url), "utf8");
const names = ["build", "build-baseline", "build-candidate",
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

test("two source-bound build runners publish all four exact-source packages", () => {
  for (const [name, section] of [["baseline-anchor", baseline], ["candidate", candidate]]) {
    assert.match(section, /^    needs: build$/m, name + " must start after source coordinator only");
    for (const content of [
      "artifact-ids: ${{ needs.build.outputs.locks-artifact }}",
      "sha256sum --check product-lock-digests.txt",
      "RUSTC_WRAPPER: sccache",
      "SCCACHE_GHA_ENABLED: \"false\"",
      "SCCACHE_LOCAL_RW_MODE: \"READ_WRITE\"",
      "NOON_WASM_PROFILE: \"release\"",
      "NOON_WASM_SKIP_OPT: \"0\"",
      "NOON_PRODUCT_HEAD_SHA: ${{ needs.build.outputs.head-sha }}",
      "NOON_PRODUCT_CANDIDATE_SHA: ${{ needs.build.outputs.candidate-sha }}",
      "uses: actions/upload-artifact@v7",
    ]) assert.ok(section.includes(content), name + " missing " + content);
  }
  for (const role of ["baseline", "anchor"]) {
    assert.ok(baseline.includes("ref: ${{ needs.build.outputs." + role + "-sha }}"),
      role + " checkout must use the exact pinned producer source");
  }
  assert.ok(baseline.includes("for noon_checkout in candidate baseline anchor; do"),
    "both baseline and anchor dependency graphs must validate");
  assert.ok(baseline.includes("NOON_PRODUCT_BASE_SHA: ${{ needs.build.outputs.anchor-sha }}"),
    "anchor identity cannot be inherited from the baseline");
  assert.ok(candidate.includes("NOON_PRODUCT_BASE_SHA: ${{ needs.build.outputs.baseline-sha }}"));
  const steps = ["Build baseline production package", "Upload baseline production package",
    "Build cumulative anchor production package", "Upload cumulative anchor production package"];
  const positions = steps.map(step => baseline.indexOf("      - name: " + step));
  assert.ok(positions.every((v, i) => v > 0 && (i === 0 || v > positions[i - 1])),
    "baseline must populate cache before same-run anchor build");
  assert.ok(baseline.includes("anchor-artifact: ${{ steps.anchor-artifact.outputs.artifact-id }}"));
});

test("lock verification does not require sccache before the compiler is installed", () => {
  for (const [name, section] of [["baseline-anchor", baseline], ["candidate", candidate]]) {
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
  assert.match(baseline, /anchor-artifact: \$\{\{ steps.anchor-artifact.outputs.artifact-id \}\}/);
  assert.match(candidate, /candidate-artifact: \$\{\{ steps.candidate-artifact.outputs.artifact-id \}\}/);
  assert.match(candidate, /candidate-fixture-artifact: \$\{\{ steps.candidate-fixture-artifact.outputs.artifact-id \}\}/);
  assert.match(baseline, /prepare baseline baseline[\s\S]*?stamp baseline/);
  assert.match(baseline, /prepare baseline anchor[\s\S]*?stamp baseline/);
  assert.match(candidate, /prepare candidate-fixture candidate[\s\S]*?stamp candidate-fixture/);
  assert.match(candidate, /prepare candidate candidate[\s\S]*?stamp candidate/);
  for (const [section, file] of [[adjacent, "baseline-artifact"],
    [adjacent, "candidate-fixture-artifact"], [cumulative, "anchor-artifact"]]) {
    const role = file.split("-artifact")[0];
    const expectedProducer = ["baseline", "anchor"].includes(role) ? "build-baseline" : "build-candidate";
    assert.ok(section.includes("needs." + expectedProducer + ".outputs." + file),
      "consumer must pin exact same-run " + file);
  }
  assert.doesNotMatch(workflow, /actions\/cache\/restore@[^\n]*[\s\S]{0,300}web\/pkg/,
    "release executables must not come from an untrusted prior run");
});

test("both measurement cohorts and the required comparator fail closed on unavailable builders", () => {
  for (const section of [adjacent, cumulative]) {
    assert.match(section, /^    needs: \[build, build-baseline, build-candidate\]$/m);
    assert.match(section, /needs.build-baseline.result == 'success'/);
    assert.match(section, /needs.build-candidate.result == 'success'/);
  }
  assert.match(compare, /needs: \[build, build-baseline, build-candidate, measure-adjacent, measure-cumulative\]/);
  for (const [label, dependency] of [["BUILD", "build"], ["BASELINE", "build-baseline"],
    ["CANDIDATE", "build-candidate"],
    ["ADJACENT", "measure-adjacent"], ["CUMULATIVE", "measure-cumulative"]]) {
    assert.ok(compare.includes("NOON_PRODUCT_" + label + "_RESULT: ${{ needs." + dependency + ".result }}"),
      "comparator does not bind actual " + dependency + " status");
  }
  for (const name of ["BUILD", "BASELINE", "CANDIDATE", "ADJACENT", "CUMULATIVE"]) {
    assert.match(compare, new RegExp('test "\\$NOON_PRODUCT_' + name + '_RESULT" = success'));
  }
  assert.match(compare, /name: Runtime, visual, latency, and FPS regression/);
  assert.match(compare, /^    if: \$\{\{ always\(\) \}\}$/m);
  assert.match(compare, /for noon_scope in adjacent cumulative/);
  assert.match(compare, /NOON_PRODUCT_MAX_LATENCY_RATIO: "1.25"/);
  assert.match(compare, /NOON_PRODUCT_MIN_FPS_RATIO: "0.80"/);
  assert.match(compare, /NOON_PRODUCT_MAX_VISUAL_DIFF_RATIO: "0.015"/);
});
