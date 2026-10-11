import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

const workflow = await readFile(
  new URL("../workflows/manim-raster-differential.yml", import.meta.url),
  "utf8",
);
const triggers = workflow.split("on:\n")[1]?.split("\npermissions:")[0];
const preflight = workflow.split("\n  preflight:\n")[1]?.split("\n  raster:\n")[0];
const oracle = workflow.split("\n  raster:\n")[1];

test("full Manim raster qualification runs nightly and by explicit dispatch, not per merge", () => {
  assert.ok(triggers, "missing event configuration");
  assert.match(triggers, /  schedule:\n    - cron: "17 6 \* \* \*"/);
  assert.match(triggers, /  workflow_dispatch:/);
  assert.match(triggers, /  pull_request:/);
  assert.doesNotMatch(triggers, /^  push:/m, "full oracle must not be retriggered after every merge");
  assert.match(oracle, /if: \$\{\{ github\.event_name == 'schedule' \|\| github\.event_name == 'workflow_dispatch' \}\}/);
  assert.match(workflow, /cancel-in-progress: \$\{\{ github\.event_name == 'pull_request' \}\}/);
});

test("relevant PRs still validate Manim policy, manifests and negative controls", () => {
  assert.ok(preflight, "missing fast PR policy job");
  assert.match(preflight, /if: \$\{\{ github\.event_name == 'pull_request' \}\}/);
  assert.match(preflight, /node scripts\/manim-raster-policy\.test\.mjs/);
  assert.match(preflight, /node --test scripts\/manim-raster-samples\.test\.mjs/);
  assert.match(preflight, /python3 -m json\.tool parity\/manim-v0\.21\/manifest\.json/);
  assert.match(preflight, /python3 -m py_compile scripts\/test_manim_raster_manifest\.py/);
  assert.match(preflight, /scripts\/special-camera-qualification\.test\.mjs/);
  assert.doesNotMatch(preflight, /build-web-demo|apt-get|pip install|playwright install/);
});

test("nightly and manually dispatched Manim gate retain full strict checks", () => {
  assert.ok(oracle, "missing full Manim raster job");
  for (const step of [
    "Install pinned ManimCE reference",
    "Build browser package",
    "Qualify pinned FollowingGraphCamera",
    "Render and compare retained Typst fixtures",
    "Enforce retained Typst raster ratchets",
    "Render and compare canonical Manim scenes",
    "Capture semantic state at raster frame times",
    "Enforce per-fixture raster ratchet",
    "Verify shared sparse and dense playback at Manim frame times",
    "Report deferred qualification failures",
  ]) {
    assert.ok(oracle.includes("      - name: " + step), "removed strict gate stage: " + step);
  }
  assert.match(oracle, /NOON_MANIM_RASTER_BACKENDS: webgpu,webgl/);
  assert.match(oracle, /if: \$\{\{ always\(\) && \(steps\.following_graph_camera\.outcome == 'failure'/);
});

test("unrelated CI policies do not select heavyweight product or browser qualification", async () => {
  const unrelated = [
    "README.md", "manim-raster-workflow-cadence.test.mjs",
    "run-report.mjs", "run-report.test.mjs", "physical-gpu-mode.test.mjs",
  ];
  for (const name of ["ci.yml", "playground-product-gate.yml"]) {
    const source = await readFile(new URL("../workflows/" + name, import.meta.url), "utf8");
    const events = source.split("on:\n")[1]?.split("\npermissions:")[0];
    const rules = [...events.matchAll(/^      - "(!?\.github\/ci\/[^\"]+)"$/gm)]
      .map(match => match[1]);
    assert.ok(rules.includes(".github/ci/**"), "new CI helpers must still select qualification");
    const selected = file => rules.reduce((included, rule) => {
      const exclude = rule.startsWith("!");
      const pattern = exclude ? rule.slice(1) : rule;
      assert.ok(pattern === ".github/ci/**" || !pattern.includes("*"), "unsupported CI policy glob");
      return pattern === ".github/ci/**" || pattern === file ? !exclude : included;
    }, false);
    for (const file of unrelated) assert.equal(selected(".github/ci/" + file), false, name + ": " + file);
    for (const file of ["wasm-build.mjs", "product-artifact.mjs", "product-cohort-topology.test.mjs", "new-helper.mjs"]) {
      assert.equal(selected(".github/ci/" + file), true, name + ": " + file);
    }
    assert.equal(["README.md", "wasm-build.mjs"].some(file => selected(".github/ci/" + file)), true,
      "a mixed policy/production change must still select qualification");
    if (name === "playground-product-gate.yml") {
      assert.equal(selected(".github/ci/gallery-optimization-topology.test.mjs"), false);
    }
  }
  const fast = await readFile(new URL("../workflows/pr-fast.yml", import.meta.url), "utf8");
  assert.match(fast, /on:\n  pull_request:\n\npermissions:/);
  assert.ok(fast.includes("node --test .github/ci/*.test.mjs"), "excluded policies must still run on every PR");
});

test("web unit-test edits stay in preflight without selecting full gallery or performance runs", async () => {
  for (const [name, count] of [["playground-product-gate.yml", 1], ["playground-cross-browser.yml", 2]]) {
    const source = await readFile(new URL("../workflows/" + name, import.meta.url), "utf8");
    const events = source.split("on:\n")[1]?.split("\npermissions:")[0];
    assert.equal(events.split('      - "web/**"\n      - "!web/**/*.test.mjs"').length - 1, count,
      name + ": exclude test sources after the broad web selector on each automatic event");
  }
});
