import assert from "node:assert/strict";
import { existsSync, readFileSync } from "node:fs";
import test from "node:test";

const cameraSource = readFileSync("web/python/_manim_camera.py", "utf8");
const manifestSource = readFileSync("web/python-compat-modules.js", "utf8");

test("obsolete retained family fade coordinator stays deleted", () => {
  assert.equal(existsSync("web/python/_manim_retained_family_fade_batch.py"), false);
  assert.doesNotMatch(manifestSource, /_manim_retained_family_fade_batch/);
  assert.doesNotMatch(cameraSource, /_manim_retained_family_fade_batch/);
});
