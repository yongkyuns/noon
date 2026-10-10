import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { spawnSync } from "node:child_process";
import test from "node:test";

const workflow = await readFile(new URL("../workflows/playground-product-gate.yml", import.meta.url), "utf8");
const jobNames = ["build", "measure-adjacent", "measure-cumulative", "compare"];
const boundaries = [...workflow.matchAll(/^  ([a-z][a-z-]*):\s*$/gm)]
  .filter(match => jobNames.includes(match[1]));
assert.deepEqual(boundaries.map(match => match[1]), jobNames,
  "all four Product Gate jobs must exist in dependency order");

function job(name) {
  const index = boundaries.findIndex(match => match[1] === name);
  assert.ok(index >= 0, "missing job " + name);
  return workflow.slice(boundaries[index].index,
    index + 1 === boundaries.length ? undefined : boundaries[index + 1].index);
}

const adjacent = job("measure-adjacent");
const cumulative = job("measure-cumulative");
const gate = job("compare");

test("both immutable-source measurement cohorts depend only on the common producer", () => {
  for (const [name, section] of [["adjacent", adjacent], ["cumulative", cumulative]]) {
    assert.match(section, /^    needs: build$/m, name + " must not wait for the other cohort");
    assert.ok(section.includes("if: ${{ needs.build.result == 'success' }}"), name + " cannot start on an unsuccessful producer");
    assert.match(section, /Require successful production builds/);
    assert.match(section, /ref: \$\{\{ needs\.build\.outputs\.candidate-sha \}\}/);
    assert.match(section, /artifact-ids: \$\{\{ needs\.build\.outputs\.candidate-artifact \}\}/);
    assert.match(section, /product-artifact\.mjs verify candidate candidate/);
    assert.match(section, /NOON_PRODUCT_HEAD_SHA: \$\{\{ needs\.build\.outputs\.head-sha \}\}/);
    assert.match(section, /NOON_PRODUCT_ANCHOR_SHA: \$\{\{ needs\.build\.outputs\.anchor-sha \}\}/);
    assert.match(section, /NOON_WASM_PROFILE: "release"/);
    assert.match(section, /NOON_WASM_SKIP_OPT: "0"/);
  }
  assert.match(adjacent, /Measure three alternating product pairs/);
  assert.doesNotMatch(adjacent, /Measure three alternating cumulative-anchor pairs/);
  assert.match(adjacent, /node candidate\/\.github\/ci\/product-artifact\.mjs verify baseline baseline/);
  assert.match(adjacent, /product-artifact\.mjs verify baseline anchor/);
  assert.match(adjacent, /Recheck release renderer initialization/);
  assert.match(cumulative, /Measure three alternating cumulative-anchor pairs/);
  assert.doesNotMatch(cumulative, /Measure three alternating product pairs/);
  assert.match(cumulative, /product-artifact\.mjs verify baseline anchor/);
  assert.match(cumulative, /NOON_PRODUCT_BASE_SHA: \$\{\{ needs\.build\.outputs\.anchor-sha \}\}/);
});

test("both cohorts upload exact same-run evidence IDs for the final comparator", () => {
  assert.match(adjacent, /evidence-artifact: \$\{\{ steps\.adjacent-evidence\.outputs\.artifact-id \}\}/);
  assert.match(cumulative, /evidence-artifact: \$\{\{ steps\.cumulative-evidence\.outputs\.artifact-id \}\}/);
  assert.match(gate, /needs: \[build, measure-adjacent, measure-cumulative\]/);
  assert.match(gate, /artifact-ids: \$\{\{ needs\.measure-adjacent\.outputs\.evidence-artifact \}\}/);
  assert.match(gate, /artifact-ids: \$\{\{ needs\.measure-cumulative\.outputs\.evidence-artifact \}\}/);
  assert.match(gate, /node candidate\/\.github\/ci\/product-artifact\.mjs verify candidate candidate/);
  assert.match(gate, /product-artifact\.mjs verify baseline baseline/);
  assert.match(gate, /product-artifact\.mjs verify baseline anchor/);
  assert.match(gate, /name: Runtime, visual, latency, and FPS regression/);
  assert.match(gate, /^    if: \$\{\{ always\(\) \}\}$/m);
  assert.match(gate, /Compare product behavior/);
  assert.match(gate, /for noon_scope in adjacent cumulative/);
  assert.match(gate, /--pairs 3/);
  assert.doesNotMatch(gate, /^      - name: Measure three alternating/m);
});

test("original strict comparison policy and the complete original five workloads remain unchanged", () => {
  for (const [name, value] of [
    ["NOON_PRODUCT_MAX_LATENCY_RATIO", "1.25"],
    ["NOON_PRODUCT_LATENCY_SLACK_MS", "350"],
    ["NOON_PRODUCT_MIN_FPS_RATIO", "0.80"],
    ["NOON_PRODUCT_MAX_VISUAL_DIFF_RATIO", "0.015"],
  ]) {
    assert.ok(gate.includes(name + ': "' + value + '"'), name + " changed");
  }
  for (const jobSection of [adjacent, cumulative]) {
    assert.match(jobSection, /noon_workloads="\$\(node \.github\/ci\/product-performance-anchor\.mjs cohorts\)"/);
    assert.match(jobSection, /for noon_pair in 1 2 3/);
    assert.match(jobSection, /if \[\[ "\$noon_pair" == 2 \]\]; then/);
    assert.match(jobSection, /NOON_PRODUCT_PAIR_POSITION/);
  }
  assert.doesNotMatch(workflow, /best[- ]of|retry.*performance/i);
});

test("blocking comparison fails closed when either upstream cohort is missing, failed, or cancelled", () => {
  const title = "      - name: Require successful production builds and both complete measurement cohorts\n";
  const parts = gate.split(title);
  assert.equal(parts.length, 2, "one blocking dependency gate");
  const block = parts[1].split("\n      - name:")[0];
  const marker = "        run: |\n";
  assert.ok(block.includes(marker), "missing executable dependency guard");
  const script = block.split(marker)[1].split("\n\n")[0].trimEnd()
    .split("\n").map(line => line.slice(10)).join("\n");
  const run = (build, adjacentResult, cumulativeResult) => spawnSync("bash", ["-e", "-c", script], {
    encoding: "utf8",
    env: {
      PATH: process.env.PATH,
      NOON_PRODUCT_BUILD_RESULT: build,
      NOON_PRODUCT_ADJACENT_RESULT: adjacentResult,
      NOON_PRODUCT_CUMULATIVE_RESULT: cumulativeResult,
    },
  });
  assert.equal(run("success", "success", "success").status, 0);
  for (const [build, adjacentResult, cumulativeResult] of [
    ["failure", "success", "success"], ["success", "failure", "success"],
    ["success", "success", "failure"], ["success", "cancelled", "success"],
    ["success", "success", "skipped"], ["", "success", "success"],
  ]) {
    const result = run(build, adjacentResult, cumulativeResult);
    assert.notEqual(result.status, 0,
      "invalid cohort states were accepted: " + [build, adjacentResult, cumulativeResult].join(","));
  }
});
