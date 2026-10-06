import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";
import { validateProductPerformanceAnchor, readProductPerformanceAnchor } from "./product-performance-anchor.mjs";

test("repository pins a post-fix cumulative product source and existing workloads", async () => {
  const anchor = await readProductPerformanceAnchor();
  assert.equal(anchor.source, "58135c4069a6f8d700dcaaee677d7c8c7a48d6c5");
  assert.deepEqual(anchor.workloads,
    ["parity-square-and-circle", "showcase-camera-follows-path"]);
});

for (const [name, mutate, expected] of [
  ["schema", value => { value.schemaVersion = 2; }, /schema/],
  ["source", value => { value.source = "main"; }, /commit SHA/],
  ["label", value => { value.label = ""; }, /label/],
  ["policy", value => { value.policy = "best-of-three"; }, /alternating-pair policy/],
  ["workloads", value => { value.workloads = ["parity-square-and-circle"]; }, /workloads changed/],
]) {
  test(`invalid anchor ${name} fails closed`, () => {
    const value = {
      schemaVersion: 1,
      source: "a".repeat(40),
      label: "fixture",
      policy: "same-run-alternating-pairs",
      workloads: ["parity-square-and-circle", "showcase-camera-follows-path"],
    };
    mutate(value);
    assert.throws(() => validateProductPerformanceAnchor(value), expected);
  });
}

test("Product Gate builds, verifies, measures, and compares the pinned anchor", async () => {
  const workflow = await readFile(new URL("../workflows/playground-product-gate.yml", import.meta.url), "utf8");
  assert.match(workflow, /Pin cumulative performance anchor/);
  assert.match(workflow, /path: anchor/);
  assert.match(workflow, /Build cumulative anchor production package/);
  assert.match(workflow, /Verify cumulative anchor source, configuration and package contents/);
  assert.match(workflow, /Measure three alternating cumulative-anchor pairs/);
  assert.match(workflow, /noon_order="anchor candidate"/);
  assert.match(workflow, /noon_order="candidate anchor"/);
  assert.match(workflow, /product-gate\/cumulative\/anchor/);
  assert.match(workflow, /product-gate\/cumulative\/camera\/anchor/);
  assert.doesNotMatch(workflow, /best[- ]of|retry.*performance/i);
});
