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
  assert.match(preflight, /python3 scripts\/test_manim_raster_manifest\.py/);
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
