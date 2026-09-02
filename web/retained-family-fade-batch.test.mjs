import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

const batchSource = readFileSync("web/python/_manim_retained_family_fade_batch.py", "utf8");
const cameraSource = readFileSync("web/python/_manim_camera.py", "utf8");
const manifestSource = readFileSync("web/python-compat-modules.js", "utf8");

test("retained family fades are represented as ordered leaf batches", () => {
  assert.match(batchSource, /class _RetainedFamilyFadeBatch/);
  assert.match(batchSource, /self\.leaves = tuple\(leaves\)/);
  assert.match(batchSource, /_compat\._leaf_mobjects\(target\)/);
  assert.match(batchSource, /len\(retained\) != len\(leaves\)/);
  assert.match(batchSource, /family fade lag_ratio requires shared retained family scheduling/);
});

test("mixed family fade batches reuse the existing retained leaf scheduler", () => {
  const schedule = batchSource.split("def _schedule_plan(")[1].split("def _scene_play(")[0];
  assert.match(schedule, /_ORIGINAL_SCHEDULE_PLAN\(/);
  assert.match(schedule, /_RetainedBatchLeafAnimation\(source\)/);
  assert.doesNotMatch(schedule, /_schedule_retained_fade\(/);
  assert.doesNotMatch(schedule, /_append_(?:vec2|scalar|presence)_track\(/);
  assert.doesNotMatch(schedule, /type\(animation\)\(/);
});

test("batch wrapper snapshots every retained leaf and restores source-level family membership", () => {
  assert.match(batchSource, /for batch in batches\s+for source in batch\.leaves/);
  assert.match(batchSource, /_normalize_retained_family_top_level/);
  assert.match(batchSource, /source\._object = old_object/);
  assert.match(batchSource, /source\._retained_object_id = old_object_id/);
  assert.match(batchSource, /source\._retained_order = old_order/);
});

test("batch adapter is installed above the canonical family transaction", () => {
  assert.match(manifestSource, /_manim_family_creation\.py[\s\S]*_manim_retained_family_fade_batch\.py/);
  const familyInstall = cameraSource.indexOf("_family_creation.install()");
  const batchInstall = cameraSource.indexOf("_retained_family_fade_batch.install()");
  assert.ok(familyInstall >= 0 && batchInstall > familyInstall);
  assert.match(batchSource, /_retained\._retained_animation_plan = _animation_plan/);
  assert.match(batchSource, /_retained\._schedule_retained_plan = _schedule_plan/);
  assert.match(batchSource, /_compat\.Scene\.play = _scene_play/);
});
