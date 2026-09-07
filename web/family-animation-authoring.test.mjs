import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { spawnSync } from "node:child_process";
import { test } from "node:test";

const pythonPath = "web/python/_manim_family_creation.py";
const pythonSource = readFileSync(pythonPath, "utf8");
const moduleManifest = readFileSync("web/python-compat-modules.js", "utf8");

test("family creation syntax module is valid Python and bundled", () => {
  const result = spawnSync("python3", ["-m", "py_compile", pythonPath], {
    encoding: "utf8",
  });
  assert.equal(result.status, 0, result.stderr || result.stdout);
  assert.match(moduleManifest, /python\/_manim_family_creation\.py/);
  assert.match(pythonSource, /class Write/);
  assert.match(pythonSource, /class Unwrite/);
});

test("family creation syntax owns no scene, scheduler, rollback, or request codec", () => {
  for (const forbidden of [
    "Scene.play",
    "retained_document",
    "familyAnimationRequest",
    "familyWriteAnimationRequest",
    "_schedule_retained_plan",
    "_authoring_checkpoint",
    "_restore_authoring_checkpoint",
    "bindRetainedNativeText",
  ]) {
    assert.equal(
      pythonSource.includes(forbidden),
      false,
      `family creation syntax must not contain ${forbidden}`,
    );
  }
});

test("Python does not serialize semantic family order or retained resource identity", () => {
  for (const forbidden of [
    "memberSlot(",
    "memberGeneration(",
    "glyph_id",
    "glyphIds",
    "atlas_id",
    "font_bytes",
  ]) {
    assert.equal(
      pythonSource.includes(forbidden),
      false,
      `Python family authoring must not contain ${forbidden}`,
    );
  }
});
