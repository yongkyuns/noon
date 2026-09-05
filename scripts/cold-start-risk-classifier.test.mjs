import assert from "node:assert/strict";
import test from "node:test";

import {
  classifyColdStartRisk,
  requiresColdStartBenchmark,
} from "./cold-start-risk-classifier.mjs";

const startupChanges = [
  ".github/workflows/pr-fast.yml",
  "scripts/build-web-demo.sh",
  "scripts/cold-start-risk-classifier.mjs",
  "scripts/cold-start-risk-classifier.test.mjs",
  "scripts/playground-cold-start.mjs",
  "web/live-authoring-bootstrap.js",
  "web/main.js",
  "web/playground-cold-start-metrics.js",
  "web/playground-cold-start-metrics.test.mjs",
  "web/python-worker.source.js",
];

test("startup and measurement changes escalate the preloaded cold-start benchmark", () => {
  const risk = classifyColdStartRisk(startupChanges);
  assert.equal(risk.coldStartBenchmark, true);
  assert.deepEqual(risk.coldStartBenchmarkPaths, startupChanges.slice().sort());
  for (const path of startupChanges) {
    assert.equal(requiresColdStartBenchmark(path), true, path);
  }
});

test("ordinary runtime and documentation changes do not escalate cold-start measurement", () => {
  const paths = [
    "README.md",
    "docs/architecture.md",
    "crates/noon-core/src/lib.rs",
    "web/playground-gallery.js",
    "scripts/browser-smoke.mjs",
    "scripts/pr-risk-classifier.mjs",
  ];
  const risk = classifyColdStartRisk(paths);
  assert.equal(risk.coldStartBenchmark, false);
  assert.deepEqual(risk.coldStartBenchmarkPaths, []);
});

test("cold-start classifier normalizes paths and de-duplicates triggers", () => {
  const risk = classifyColdStartRisk([
    "./web/main.js",
    "web\\main.js",
    "web/main.js",
    "",
  ]);
  assert.equal(risk.coldStartBenchmark, true);
  assert.deepEqual(risk.coldStartBenchmarkPaths, ["web/main.js"]);
});

test("cold-start classifier rejects non-array inputs", () => {
  assert.throws(() => classifyColdStartRisk(null), /must be an array/);
});
