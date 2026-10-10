import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { spawnSync } from "node:child_process";
import test from "node:test";

const workflow = await readFile(new URL("../workflows/playground-product-gate.yml", import.meta.url), "utf8");
const jobNames = ["build", "build-baseline", "build-candidate", "measure-adjacent", "measure-cumulative", "compare"];
const boundaries = [...workflow.matchAll(/^  ([a-z][a-z-]*):\s*$/gm)]
  .filter(match => jobNames.includes(match[1]));
assert.deepEqual(boundaries.map(match => match[1]), jobNames,
  "all six Product Gate jobs must exist in dependency order");

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
    assert.match(section, /^    needs: \[build, build-baseline, build-candidate\]$/m, name + " must not wait for the other cohort");
    assert.ok(section.includes("if: ${{ always() && needs.build.result == 'success' && needs.build-baseline.result == 'success' && needs.build-candidate.result == 'success' }}"), name + " cannot start on an unsuccessful producer");
    assert.match(section, /Require successful production builds/);
    assert.match(section, /ref: \$\{\{ needs\.build\.outputs\.candidate-sha \}\}/);
    assert.match(section, /artifact-ids: \$\{\{ needs\.build-candidate\.outputs\.candidate-artifact \}\}/);
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
  assert.match(gate, /needs: \[build, build-baseline, build-candidate, measure-adjacent, measure-cumulative\]/);
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

test("blocking comparison fails closed for any failed, cancelled, or missing build/cohort job", () => {
  const title = "      - name: Require successful production builds and both complete measurement cohorts\n";
  const parts = gate.split(title);
  assert.equal(parts.length, 2, "one blocking dependency gate");
  const block = parts[1].split("\n      - name:")[0];
  const marker = "        run: |\n";
  assert.ok(block.includes(marker), "missing executable dependency guard");
  const script = block.split(marker)[1].split("\n\n")[0].trimEnd()
    .split("\n").map(line => line.slice(10)).join("\n");
  const results = ["BUILD", "BASELINE", "CANDIDATE", "ADJACENT", "CUMULATIVE"];
  for (const name of results) assert.ok(block.includes("NOON_PRODUCT_" + name + "_RESULT"), name);
  const run = values => spawnSync("bash", ["-e", "-c", script], {
    encoding: "utf8",
    env: {
      PATH: process.env.PATH,
      ...Object.fromEntries(results.map((name, index) => ["NOON_PRODUCT_" + name + "_RESULT", values[index]])),
    },
  });
  const success = results.map(() => "success");
  assert.equal(run(success).status, 0);
  for (const index of results.keys()) {
    for (const bad of ["failure", "cancelled", "skipped", ""]) {
      const statuses = [...success];
      statuses[index] = bad;
      assert.notEqual(run(statuses).status, 0,
        "failed or missing producer was accepted: " + results[index] + " " + bad);
    }
  }
});
